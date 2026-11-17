use crate::jira::Client;
use gpui_kit::{Image, ImageFormat};
use std::{
    cell::{OnceCell, RefCell},
    collections::HashMap,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

type Preview = Result<Arc<Image>, String>;
struct Entry {
    result: Option<Preview>,
    used: Instant,
    retry: Instant,
}
struct Job {
    key: String,
    id: String,
    client: Client,
}
type Events = mpsc::Receiver<(String, Preview)>;
#[derive(Default)]
pub struct Previews {
    entries: RefCell<HashMap<String, Entry>>,
    jobs: OnceCell<mpsc::SyncSender<Job>>,
    events: RefCell<Option<Events>>,
}
impl Previews {
    pub fn get(&self, client: &Client, id: &str) -> Option<Preview> {
        let key = format!(
            "{}|{}|{}",
            client.config.jira.base_url, client.config.jira.email, id
        );
        let mut entries = self.entries.borrow_mut();
        if entries.len() >= 32 && !entries.contains_key(&key) {
            let oldest = entries
                .iter()
                .filter(|(_, e)| e.result.is_some())
                .min_by_key(|(_, e)| e.used)
                .map(|(k, _)| k.clone());
            entries.remove(&oldest?);
        }
        let now = Instant::now();
        let entry = entries.entry(key.clone()).or_insert(Entry {
            result: None,
            used: now,
            retry: now,
        });
        entry.used = now;
        if now >= entry.retry {
            let jobs = self.jobs.get_or_init(|| {
                let (tx, rx) = mpsc::sync_channel::<Job>(32);
                let (out, events) = mpsc::channel();
                *self.events.borrow_mut() = Some(events);
                std::thread::spawn(move || {
                    while let Ok(job) = rx.recv() {
                        let result = job
                            .client
                            .attachment_preview(&job.id)
                            .and_then(|bytes| normalize(&bytes))
                            .map(|bytes| Arc::new(Image::from_bytes(ImageFormat::Png, bytes)))
                            .map_err(|_| "Preview unavailable".to_string());
                        if out.send((job.key, result)).is_err() {
                            break;
                        }
                    }
                });
                tx
            });
            if jobs
                .try_send(Job {
                    key,
                    id: id.into(),
                    client: client.clone(),
                })
                .is_ok()
            {
                entry.retry = now + Duration::from_secs(300);
            }
        }
        entry.result.clone()
    }
    pub fn poll(&self) -> bool {
        let mut changed = false;
        if let Some(events) = self.events.borrow().as_ref() {
            while let Ok((key, result)) = events.try_recv() {
                if let Some(entry) = self.entries.borrow_mut().get_mut(&key) {
                    entry.result = Some(result);
                    changed = true;
                }
            }
        }
        changed
    }
}
fn normalize(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode()?.thumbnail(640, 400);
    let mut output = std::io::Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Png)?;
    Ok(output.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_rejects_nonimages_and_bounds_dimensions() {
        assert!(normalize(b"not an image").is_err());
        let source = image::DynamicImage::new_rgb8(1600, 1000);
        let mut bytes = std::io::Cursor::new(Vec::new());
        source
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let result = image::load_from_memory(&normalize(&bytes.into_inner()).unwrap()).unwrap();
        assert_eq!((result.width(), result.height()), (640, 400));
    }
}
