param([Parameter(Mandatory=$true)][int]$TargetProcessId)
$ErrorActionPreference='Stop'
Add-Type @"
using System; using System.Runtime.InteropServices;
public class TabSaveChord {
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint process);
 [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
 [DllImport("user32.dll")] static extern bool AttachThreadInput(uint a,uint b,bool attach);
 [DllImport("user32.dll")] static extern bool GetKeyboardState(byte[] state);
 [DllImport("user32.dll")] static extern bool SetKeyboardState(byte[] state);
 [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h,uint message,IntPtr w,IntPtr l);
 public static void Send(IntPtr h,uint expected) {
  uint process; uint target=GetWindowThreadProcessId(h,out process);
  if(process!=expected) throw new Exception("Wrong target process");
  uint current=GetCurrentThreadId();
  if(!AttachThreadInput(current,target,true)) throw new Exception("Cannot attach test keyboard queue");
  byte[] saved=new byte[256];
  bool captured=false;
  try {
   if(!GetKeyboardState(saved)) throw new Exception("Cannot read keyboard state");
   captured=true;
   byte[] state=(byte[])saved.Clone(); state[0x11]=0x80; state[0xa2]=0x80;
   if(!SetKeyboardState(state)) throw new Exception("Cannot set test modifier");
   PostMessage(h,0x100,(IntPtr)0x0d,(IntPtr)0x001c0001);
   System.Threading.Thread.Sleep(250);
   PostMessage(h,0x101,(IntPtr)0x0d,new IntPtr(unchecked((int)0xc01c0001)));
   System.Threading.Thread.Sleep(100);
  } finally { if(captured) SetKeyboardState(saved); AttachThreadInput(current,target,false); }
 }
}
"@
$process=Get-Process -Id $TargetProcessId -ErrorAction Stop
if($process.ProcessName -ne 'not-bad-jira' -or $process.MainWindowHandle -eq 0){throw 'Expected exact test app process'}
[TabSaveChord]::Send($process.MainWindowHandle,[uint32]$TargetProcessId)
