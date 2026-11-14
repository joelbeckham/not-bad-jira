use crate::jira::Client;
use gpui_kit::{Image, ImageFormat};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    cell::{OnceCell, RefCell},
    collections::HashMap,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime},
};

const TTL: Duration = Duration::from_secs(24 * 60 * 60);
const RETRY: Duration = Duration::from_secs(5 * 60);
const MAX_IMAGE_BYTES: usize = 64 * 1024;
const MEMORY_ENTRIES: usize = 256;
const DISK_ENTRIES: usize = 512;
const DISK_BYTES: u64 = 16 * 1024 * 1024;

struct Entry {
    image: Option<Arc<Image>>,
    loading: bool,
    retry: Instant,
    used: Instant,
    authorized: bool,
}

struct Job {
    key: String,
    address: String,
    client: Option<Client>,
}

enum Event {
    Ready(String, Vec<u8>),
    Done(String, Duration),
}

#[derive(Default)]
pub struct AvatarCache {
    entries: RefCell<HashMap<String, Entry>>,
    jobs: OnceCell<mpsc::SyncSender<Job>>,
    events: RefCell<Option<mpsc::Receiver<Event>>>,
}

pub fn avatar_url(user: &Value) -> Option<&str> {
    ["48x48", "32x32", "24x24", "16x16"]
        .iter()
        .find_map(|size| {
            user["avatarUrls"][size]
                .as_str()
                .filter(|url| !url.is_empty())
        })
}

fn key(account: &crate::config::Config, address: &str) -> String {
    cache_key(
        &format!("{}|{}", account.jira.base_url, account.jira.email),
        address,
    )
}

fn cache_key(account: &str, address: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(account.as_bytes());
    hash.update([0]);
    hash.update(address.as_bytes());
    format!("{:x}", hash.finalize())
}

impl AvatarCache {
    // Called during rendering: only memory lookup and a non-blocking queue send.
    pub fn get(
        &self,
        account: Option<&crate::config::Config>,
        client: Option<&Client>,
        user: &Value,
    ) -> Option<Arc<Image>> {
        let account = account?;
        let address = avatar_url(user)?;
        let key = key(account, address);
        let now = Instant::now();
        let mut entries = self.entries.borrow_mut();
        if !entries.contains_key(&key) && entries.len() >= MEMORY_ENTRIES {
            let oldest = entries
                .iter()
                .filter(|(_, entry)| !entry.loading)
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| key.clone());
            entries.remove(&oldest?);
        }
        let entry = entries.entry(key.clone()).or_insert_with(|| Entry {
            image: None,
            loading: false,
            retry: now,
            used: now,
            authorized: false,
        });
        entry.used = now;
        if !entry.loading && (now >= entry.retry || (client.is_some() && !entry.authorized)) {
            let sender = self.jobs.get_or_init(|| {
                let root = crate::config::cache_path()
                    .parent()
                    .unwrap()
                    .join("avatars");
                let (jobs, events) = workers(root);
                *self.events.borrow_mut() = Some(events);
                jobs
            });
            entry.loading = sender
                .try_send(Job {
                    key,
                    address: address.into(),
                    client: client.cloned(),
                })
                .is_ok();
            entry.authorized = client.is_some();
            if !entry.loading {
                entry.retry = now + Duration::from_secs(1);
            }
        }
        entry.image.clone()
    }

    pub fn poll(&self) -> bool {
        let mut changed = false;
        if let Some(events) = self.events.borrow().as_ref() {
            while let Ok(event) = events.try_recv() {
                changed |= self.apply(event);
            }
        }
        changed
    }

    fn apply(&self, event: Event) -> bool {
        let mut entries = self.entries.borrow_mut();
        match event {
            Event::Ready(key, bytes) => {
                if let Some(entry) = entries.get_mut(&key) {
                    entry.image = Some(Arc::new(Image::from_bytes(ImageFormat::Png, bytes)));
                    return true;
                }
            }
            Event::Done(key, delay) => {
                if let Some(entry) = entries.get_mut(&key) {
                    entry.loading = false;
                    entry.retry = Instant::now() + delay;
                    return true;
                }
            }
        }
        false
    }
}

fn workers(root: PathBuf) -> (mpsc::SyncSender<Job>, mpsc::Receiver<Event>) {
    let (jobs_tx, jobs_rx) = mpsc::sync_channel::<Job>(32);
    let (fetch_tx, fetch_rx) = mpsc::sync_channel::<Job>(32);
    let (events_tx, events_rx) = mpsc::channel();
    let fetch_rx = Arc::new(Mutex::new(fetch_rx));
    for _ in 0..2 {
        let receiver = fetch_rx.clone();
        let events = events_tx.clone();
        let root = root.clone();
        std::thread::spawn(move || {
            loop {
                let Ok(job) = receiver.lock().unwrap().recv() else {
                    break;
                };
                let result = job
                    .client
                    .as_ref()
                    .and_then(|client| client.avatar_bytes(&job.address).ok())
                    .and_then(|bytes| normalize(&bytes).ok());
                let delay = if let Some(bytes) = result {
                    let _ = save(&root, &job.key, &bytes);
                    let _ = events.send(Event::Ready(job.key.clone(), bytes));
                    TTL
                } else {
                    RETRY
                };
                let _ = events.send(Event::Done(job.key, delay));
            }
        });
    }
    // Disk hits must not sit behind a slow or offline HTTP request.
    std::thread::spawn(move || {
        while let Ok(job) = jobs_rx.recv() {
            load_or_enqueue(&root, job, &fetch_tx, &events_tx);
        }
    });
    (jobs_tx, events_rx)
}

fn load_or_enqueue(
    root: &Path,
    job: Job,
    fetch: &mpsc::SyncSender<Job>,
    events: &mpsc::Sender<Event>,
) {
    if let Some((bytes, remaining)) = load(root, &job.key) {
        let _ = events.send(Event::Ready(job.key.clone(), bytes));
        if !remaining.is_zero() {
            let _ = events.send(Event::Done(job.key, remaining));
            return;
        }
    }
    if job.client.is_none() {
        let _ = events.send(Event::Done(job.key, RETRY));
        return;
    }
    if let Err(error) = fetch.try_send(job) {
        let (mpsc::TrySendError::Full(job) | mpsc::TrySendError::Disconnected(job)) = error;
        let _ = events.send(Event::Done(job.key, RETRY));
    }
}

fn normalize(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(1024);
    limits.max_alloc = Some(8 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()?
        .resize_to_fill(48, 48, image::imageops::FilterType::Triangle);
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png)?;
    Ok(png.into_inner())
}

fn load(root: &Path, key: &str) -> Option<(Vec<u8>, Duration)> {
    let file = std::fs::File::open(root.join(format!("{key}.png"))).ok()?;
    let age = file
        .metadata()
        .ok()?
        .modified()
        .ok()?
        .elapsed()
        .unwrap_or_default();
    let mut bytes = Vec::new();
    file.take((MAX_IMAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return None;
    }
    Some((normalize(&bytes).ok()?, TTL.saturating_sub(age)))
}

fn save(root: &Path, key: &str, bytes: &[u8]) -> anyhow::Result<()> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    std::fs::create_dir_all(root)?;
    let temp = root.join(format!(
        "{key}.{}.{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> anyhow::Result<()> {
        std::fs::write(&temp, bytes)?;
        std::fs::rename(&temp, root.join(format!("{key}.png")))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result?;
    prune(root, DISK_ENTRIES, DISK_BYTES);
    Ok(())
}

fn prune(root: &Path, max_entries: usize, max_bytes: u64) {
    let Ok(directory) = std::fs::read_dir(root) else {
        return;
    };
    let mut files: Vec<_> = directory
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            let hash = name.strip_suffix(".png")?;
            if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            Some((
                metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                metadata.len(),
                entry.path(),
            ))
        })
        .collect();
    files.sort_by_key(|file| file.0);
    let mut total: u64 = files.iter().map(|file| file.1).sum();
    let mut count = files.len();
    for (_, length, path) in files {
        if count <= max_entries && total <= max_bytes {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total = total.saturating_sub(length);
            count -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "not-bad-jira-avatar-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            for entry in std::fs::read_dir(&self.0).unwrap().flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
            let _ = std::fs::remove_dir(&self.0);
        }
    }
    fn png() -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(64, 32, image::Rgba([100, 140, 200, 255]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }
    fn job(key: String) -> Job {
        Job {
            key,
            address: "https://avatar.example/test.png".into(),
            client: Some(Client::new(
                crate::config::Config::initial(
                    "https://jira.example".into(),
                    "test@example.com".into(),
                    "T".into(),
                ),
                "fixture-token".into(),
            )),
        }
    }

    #[test]
    fn cache_keys_isolate_accounts_and_avatar_versions() {
        assert_ne!(
            cache_key("account-a", "https://avatar/1"),
            cache_key("account-b", "https://avatar/1")
        );
        assert_ne!(
            cache_key("account-a", "https://avatar/1"),
            cache_key("account-a", "https://avatar/2")
        );
        assert_eq!(cache_key("account-a", "https://avatar/1").len(), 64);
        assert_eq!(
            avatar_url(&serde_json::json!({"avatarUrls":{"16x16":"small","48x48":"large"}})),
            Some("large")
        );
    }

    #[test]
    fn disk_hits_bypass_http_and_stale_images_show_while_refreshing() {
        let directory = Directory::new();
        let key = cache_key("account", "avatar");
        save(&directory.0, &key, &normalize(&png()).unwrap()).unwrap();
        let (fetch, pending) = mpsc::sync_channel(4);
        let (events, ready) = mpsc::channel();
        let mut offline = job(key.clone());
        offline.client = None;
        load_or_enqueue(&directory.0, offline, &fetch, &events);
        assert!(matches!(ready.recv().unwrap(), Event::Ready(_, _)));
        assert!(matches!(ready.recv().unwrap(), Event::Done(_, delay) if !delay.is_zero()));
        assert!(pending.try_recv().is_err());

        std::fs::File::options()
            .write(true)
            .open(directory.0.join(format!("{key}.png")))
            .unwrap()
            .set_modified(SystemTime::now() - TTL - Duration::from_secs(1))
            .unwrap();
        load_or_enqueue(&directory.0, job(key.clone()), &fetch, &events);
        assert!(matches!(ready.recv().unwrap(), Event::Ready(_, _)));
        assert_eq!(pending.recv().unwrap().key, key);
    }

    #[test]
    fn corrupt_images_refetch_and_size_limits_reject_large_images() {
        let directory = Directory::new();
        let key = cache_key("account", "avatar");
        std::fs::write(directory.0.join(format!("{key}.png")), b"not an image").unwrap();
        assert!(load(&directory.0, &key).is_none());
        let (fetch, pending) = mpsc::sync_channel(4);
        let (events, _) = mpsc::channel();
        load_or_enqueue(&directory.0, job(key.clone()), &fetch, &events);
        assert_eq!(pending.recv().unwrap().key, key);
        let normalized = normalize(&png()).unwrap();
        let image = image::load_from_memory(&normalized).unwrap();
        assert_eq!((image.width(), image.height()), (48, 48));
        let mut large = Cursor::new(Vec::new());
        image::RgbaImage::new(1025, 1)
            .write_to(&mut large, image::ImageFormat::Png)
            .unwrap();
        assert!(normalize(&large.into_inner()).is_err());
    }

    #[test]
    fn failed_refresh_keeps_the_cached_image_and_backs_off() {
        let cache = AvatarCache::default();
        let now = Instant::now();
        cache.entries.borrow_mut().insert(
            "key".into(),
            Entry {
                image: None,
                loading: true,
                retry: now,
                used: now,
                authorized: true,
            },
        );
        assert!(cache.apply(Event::Ready("key".into(), normalize(&png()).unwrap())));
        let original = cache.entries.borrow()["key"].image.clone().unwrap();
        cache.apply(Event::Done("key".into(), RETRY));
        let entries = cache.entries.borrow();
        assert!(Arc::ptr_eq(
            &original,
            entries["key"].image.as_ref().unwrap()
        ));
        assert!(!entries["key"].loading);
        assert!(entries["key"].retry > now);
    }

    #[test]
    fn connecting_enables_fetch_immediately_and_pending_requests_are_deduplicated() {
        let cache = AvatarCache::default();
        let (sender, pending) = mpsc::sync_channel(4);
        assert!(cache.jobs.set(sender).is_ok());
        let client = job("unused".into()).client.unwrap();
        let user = serde_json::json!({"avatarUrls":{"48x48":"https://avatar.example/photo.png"}});
        assert!(cache.get(Some(&client.config), None, &user).is_none());
        let offline = pending.try_recv().unwrap();
        assert!(offline.client.is_none());
        cache.apply(Event::Done(offline.key, RETRY));
        assert!(
            cache
                .get(Some(&client.config), Some(&client), &user)
                .is_none()
        );
        assert!(pending.try_recv().unwrap().client.is_some());
        cache.get(Some(&client.config), Some(&client), &user);
        assert!(pending.try_recv().is_err());
    }

    #[test]
    fn disk_eviction_preserves_other_files_and_enforces_budget() {
        let directory = Directory::new();
        for i in 0..3 {
            std::fs::write(
                directory
                    .0
                    .join(format!("{}.png", cache_key("account", &i.to_string()))),
                [1; 10],
            )
            .unwrap();
        }
        std::fs::write(directory.0.join("notes.txt"), b"keep").unwrap();
        prune(&directory.0, 2, 10);
        let remaining: Vec<_> = std::fs::read_dir(&directory.0)
            .unwrap()
            .flatten()
            .filter(|file| file.path().extension().is_some_and(|ext| ext == "png"))
            .collect();
        assert_eq!(remaining.len(), 1);
        assert_eq!(
            std::fs::read(directory.0.join("notes.txt")).unwrap(),
            b"keep"
        );
    }
}
