# Runs a released WeathaDock on desktops set up in different ways and reports, every few
# seconds, where its widget is among the other windows and whether anything of it can be
# seen on the screen. Only for finding out why the widget disappears on some desktops.
param([string]$Version = '1.1.16')
$ErrorActionPreference = 'Continue'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -Namespace Lab -Name Win -MemberDefinition @'
public struct Rect { public int Left, Top, Right, Bottom; }
[DllImport("user32.dll")] public static extern IntPtr GetTopWindow(IntPtr parent);
[DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr window, uint cmd);
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
[DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
[DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr window, int index);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr window, System.Text.StringBuilder text, int max);
[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string title);
[DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr window, uint msg, IntPtr w, IntPtr l, uint flags, uint timeout, out IntPtr result);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern bool SystemParametersInfo(uint action, uint param, string value, uint flags);
[DllImport("user32.dll")] public static extern bool SetSysColors(int count, int[] elements, int[] colors);
[DllImport("user32.dll")] public static extern int GetSysColor(int index);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
[DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
[DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
[DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr window, int attribute, out int value, int size);

public static string Class(IntPtr h) { var s = new System.Text.StringBuilder(256); GetClassName(h, s, 256); return s.ToString(); }

// The stack of top-level windows, top first: the app's own, the desktop's, and any other
// that fills the screen. The number is the place in the stack.
public static string Stack(uint app) {
  var o = new System.Text.StringBuilder();
  int sw = GetSystemMetrics(0), sh = GetSystemMetrics(1), i = 0;
  for (IntPtr h = GetTopWindow(IntPtr.Zero); h != IntPtr.Zero; h = GetWindow(h, 2), i++) {
    if (!IsWindowVisible(h)) continue;
    uint pid; GetWindowThreadProcessId(h, out pid);
    string c = Class(h); Rect r; GetWindowRect(h, out r);
    bool desk = c == "Progman" || c == "WorkerW";
    bool full = !IsIconic(h) && r.Right - r.Left >= sw && r.Bottom - r.Top >= sh;
    if (pid != app && !desk && !full) continue;
    int ex = GetWindowLong(h, -20), cloak = 0; DwmGetWindowAttribute(h, 14, out cloak, 4);
    o.Append(i + ":" + (pid == app ? "APP/" : "") + c + " " + (r.Right - r.Left) + "x" + (r.Bottom - r.Top) + "@" + r.Left + "," + r.Top
      + ((ex & 8) != 0 ? " top" : "") + ((ex & 0x80000) != 0 ? " layered" : "") + ((ex & 0x20) != 0 ? " through" : "")
      + (cloak != 0 ? " cloaked" : "") + (IsIconic(h) ? " min" : "")
      + (FindWindowEx(h, IntPtr.Zero, "SHELLDLL_DefView", null) != IntPtr.Zero ? " +icons" : "") + " | ");
  }
  o.Append("fg=" + Class(GetForegroundWindow()));
  return o.ToString();
}

// Where the app's widget is: its largest visible window at least 61 px tall.
public static string Widget(uint app) {
  string found = ""; int best = 0;
  for (IntPtr h = GetTopWindow(IntPtr.Zero); h != IntPtr.Zero; h = GetWindow(h, 2)) {
    uint pid; Rect r; GetWindowThreadProcessId(h, out pid);
    if (pid == app && IsWindowVisible(h) && GetWindowRect(h, out r) && r.Right - r.Left > 60 && r.Bottom - r.Top >= 61 && (r.Right - r.Left) * (r.Bottom - r.Top) > best) {
      best = (r.Right - r.Left) * (r.Bottom - r.Top);
      found = r.Left + " " + r.Top + " " + r.Right + " " + r.Bottom;
    }
  }
  return found;
}

// Ask the desktop to put its picture in a window of its own, the way wallpaper changes with a
// fade and live-wallpaper programs do.
public static string Split() {
  IntPtr p = FindWindow("Progman", null), res;
  SendMessageTimeout(p, 0x052C, IntPtr.Zero, IntPtr.Zero, 0, 1000, out res);
  SendMessageTimeout(p, 0x052C, new IntPtr(0xD), new IntPtr(1), 0, 1000, out res);
  return "asked " + p;
}

public static void Focus(IntPtr h) {
  keybd_event(0x12, 0, 0, UIntPtr.Zero); SetForegroundWindow(h); keybd_event(0x12, 0, 2, UIntPtr.Zero);
}
'@
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace Lab {
  [StructLayout(LayoutKind.Sequential)] public struct R { public int Left, Top, Right, Bottom; }
  [ComImport, Guid("B92B56A9-8B55-4E14-9A89-0199BBB6F93B"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  public interface IDesktopWallpaper {
    void SetWallpaper([MarshalAs(UnmanagedType.LPWStr)] string monitor, [MarshalAs(UnmanagedType.LPWStr)] string wallpaper);
    [return: MarshalAs(UnmanagedType.LPWStr)] string GetWallpaper([MarshalAs(UnmanagedType.LPWStr)] string monitor);
    [return: MarshalAs(UnmanagedType.LPWStr)] string GetMonitorDevicePathAt(uint index);
    uint GetMonitorDevicePathCount();
    R GetMonitorRECT([MarshalAs(UnmanagedType.LPWStr)] string monitor);
    void SetBackgroundColor(uint color);
    uint GetBackgroundColor();
    void SetPosition(int position);
    int GetPosition();
    void SetSlideshow(IntPtr items);
    IntPtr GetSlideshow();
    void SetSlideshowOptions(int options, uint tick);
    void GetSlideshowOptions(out int options, out uint tick);
    void AdvanceSlideshow([MarshalAs(UnmanagedType.LPWStr)] string monitor, int direction);
    int GetStatus();
    void Enable([MarshalAs(UnmanagedType.Bool)] bool enable);
  }
  [ComImport, Guid("C2CF3110-460E-4fc1-B9D0-8A1C0C9CC4BD")] public class DesktopWallpaperClass { }
  public static class Paper {
    static IDesktopWallpaper New() { return (IDesktopWallpaper)new DesktopWallpaperClass(); }
    static string Try(Func<string> f) { try { return f(); } catch (Exception e) { return "FAILED 0x" + e.HResult.ToString("X8"); } }
    // What Windows has on record for the desktop.
    public static string Describe() {
      return Try(() => {
        var d = New(); var o = "";
        uint n = d.GetMonitorDevicePathCount();
        for (uint i = 0; i < n; i++) {
          string id = d.GetMonitorDevicePathAt(i);
          o += "monitor" + i + " rect=" + Try(() => { var r = d.GetMonitorRECT(id); return r.Left + "," + r.Top + "," + r.Right + "," + r.Bottom; })
            + " picture=[" + Try(() => d.GetWallpaper(id)) + "]; ";
        }
        o += "picture(all)=[" + Try(() => d.GetWallpaper(null)) + "]; position=" + Try(() => d.GetPosition().ToString())
          + "; colour=" + Try(() => "0x" + d.GetBackgroundColor().ToString("X6")) + "; status=" + Try(() => d.GetStatus().ToString());
        return o;
      });
    }
    public static string Picture(string path) { return Try(() => { New().SetWallpaper(null, path); return "ok"; }); }
    public static string Enable(bool on) { return Try(() => { New().Enable(on); return "ok"; }); }
    public static string Colour(uint colour) { return Try(() => { New().SetBackgroundColor(colour); return "ok"; }); }
  }
}
'@

$os = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
"LAB os: $($os.ProductName) $($os.DisplayVersion) build $($os.CurrentBuildNumber).$($os.UBR); app $Version"
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
"LAB screen: $($screen.Width)x$($screen.Height); screens: " + [System.Windows.Forms.Screen]::AllScreens.Count
"LAB desktop at the start: " + [Lab.Win]::Stack(0)
"LAB record at the start: " + [Lab.Paper]::Describe()

$setup = Join-Path $env:TEMP "WeathaDock_${Version}_x64-setup.exe"
Invoke-WebRequest "https://github.com/awpmap/weathadock/releases/download/v$Version/WeathaDock_${Version}_x64-setup.exe" -OutFile $setup
Start-Process $setup -ArgumentList '/S' -Wait
$exe = Join-Path $env:LOCALAPPDATA 'WeathaDock\weathadock.exe'
"LAB installed: " + (Test-Path $exe) + " " + $(if (Test-Path $exe) { (Get-Item $exe).VersionInfo.ProductVersion })
if (-not (Test-Path $exe)) { Write-Error "The installer did not put the app in place."; exit 1 }
$cfg = Join-Path $env:APPDATA 'app.weathadock.desktop'
New-Item -ItemType Directory -Force -Path $cfg | Out-Null
$log = Join-Path $env:TEMP 'lab.log'
$env:WEATHADOCK_LOG = $log

# Pictures for the desktop: plain black, and one that is see-through all over with white
# stored underneath (Windows shows such a picture as black).
$own = (Get-ItemProperty 'HKCU:\Control Panel\Desktop').WallPaper
$dir = Join-Path $env:TEMP 'lab-papers'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$blackPng = Join-Path $dir 'black.png'
$clearPng = Join-Path $dir 'clear.png'
$whitePng = Join-Path $dir 'white.png'
$bmp = New-Object System.Drawing.Bitmap $screen.Width, $screen.Height
$g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::White); $g.Dispose(); $bmp.Save($whitePng); $bmp.Dispose()
$bmp = New-Object System.Drawing.Bitmap $screen.Width, $screen.Height
$g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::Black); $g.Dispose(); $bmp.Save($blackPng); $bmp.Dispose()
$bmp = New-Object System.Drawing.Bitmap $screen.Width, $screen.Height, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::FromArgb(0, 255, 255, 255)); $g.Dispose(); $bmp.Save($clearPng); $bmp.Dispose()

$jpeg = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object { $_.MimeType -eq 'image/jpeg' }
$quality = New-Object System.Drawing.Imaging.EncoderParameters 1
$quality.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter ([System.Drawing.Imaging.Encoder]::Quality), 50L

# How light a part of the screen is: the average and the lightest spot (0 to 255) of a small copy.
function Get-Light([int]$left, [int]$top, [int]$wide, [int]$high) {
  if ($wide -lt 2 -or $high -lt 2) { return 'none' }
  $shot = New-Object System.Drawing.Bitmap $wide, $high
  $g = [System.Drawing.Graphics]::FromImage($shot); $g.CopyFromScreen($left, $top, 0, 0, $shot.Size); $g.Dispose()
  $small = New-Object System.Drawing.Bitmap 64, 64
  $g = [System.Drawing.Graphics]::FromImage($small); $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $g.DrawImage($shot, 0, 0, 64, 64); $g.Dispose()
  $data = $small.LockBits((New-Object System.Drawing.Rectangle 0, 0, 64, 64), [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $bytes = New-Object byte[] (64 * 64 * 4)
  [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $bytes.Length)
  $small.UnlockBits($data); $small.Dispose(); $shot.Dispose()
  $sum = 0; $max = 0
  for ($i = 1; $i -lt $bytes.Length; $i += 4) { $sum += $bytes[$i]; if ($bytes[$i] -gt $max) { $max = $bytes[$i] } }
  "{0:N1}/{1}" -f ($sum / 4096), $max
}

function Save-Shot($name, [int]$left, [int]$top, [int]$wide, [int]$high) {
  $shot = New-Object System.Drawing.Bitmap $wide, $high
  $g = [System.Drawing.Graphics]::FromImage($shot); $g.CopyFromScreen($left, $top, 0, 0, $shot.Size); $g.Dispose()
  $file = Join-Path $env:TEMP "$name.jpg"
  $shot.Save($file, $jpeg, $quality); $shot.Dispose()
  Write-Output "$name-BEGIN"
  [Convert]::ToBase64String([IO.File]::ReadAllBytes($file)) -split '(.{4000})' | Where-Object { $_ }
  Write-Output "$name-END"
}

function Set-Desk($kind) {
  [Lab.Paper]::Enable($true) | Out-Null
  switch ($kind) {
    'own' { [Lab.Win]::SystemParametersInfo(0x0014, 0, $own, 3) | Out-Null }
    'black' { [Lab.Win]::SystemParametersInfo(0x0014, 0, '', 3) | Out-Null; [Lab.Win]::SetSysColors(1, @(1), @(0)) | Out-Null }
    'off' { "LAB   a white picture first: " + [Lab.Paper]::Picture($whitePng); Start-Sleep -Seconds 2; "LAB   switch the picture off: " + [Lab.Paper]::Enable($false) + ", colour: " + [Lab.Paper]::Colour(0) }
    'blackpic' { "LAB   picture through the newer way: " + [Lab.Paper]::Picture($blackPng) }
    'clearpng' { [Lab.Win]::SystemParametersInfo(0x0014, 0, $clearPng, 3) | Out-Null }
  }
  Start-Sleep -Seconds 3
}

$helper = New-Object System.Windows.Forms.Form
$helper.Text = 'Another app'; $helper.StartPosition = 'Manual'; $helper.Location = New-Object System.Drawing.Point 20, 260; $helper.Size = New-Object System.Drawing.Size 260, 160

function Sample($label, $app) {
  [System.Windows.Forms.Application]::DoEvents()
  $where = [Lab.Win]::Widget([uint32]$app.Id)
  $light = 'none'
  if ($where) { $r = $where -split ' ' | ForEach-Object { [int]$_ }; $light = Get-Light $r[0] $r[1] ($r[2] - $r[0]) ($r[3] - $r[1]) }
  $desk = Get-Light ([int]($screen.Width / 2) - 60) ([int]($screen.Height / 2) - 60) 120 120
  "LAB   {0,-22} alive={1} widget=[{2}] light={3} desk={4} :: {5}" -f $label, (-not $app.HasExited), $where, $light, $desk, [Lab.Win]::Stack([uint32]$app.Id)
}

function Run($letter, $desk, $settings, $note) {
  Get-Process weathadock -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Seconds 2
  "LAB $letter ===== $note"
  Set-Desk $desk
  "LAB   record: " + [Lab.Paper]::Describe()
  "LAB   registry: WallPaper=[" + (Get-ItemProperty 'HKCU:\Control Panel\Desktop').WallPaper + "] desktop colour=0x" + ([Lab.Win]::GetSysColor(1)).ToString('X6')
  ('{"locations":[{"zip":"60601","city":"Chicago","state":"IL","lat":41.8858,"lon":-87.6181}],"activeZip":"60601","launchAtLogin":false,"alertNotifications":false' + $settings + '}') | Set-Content (Join-Path $cfg 'settings.json')
  Remove-Item $log -ErrorAction SilentlyContinue
  Remove-Item (Join-Path $cfg 'crash.log') -ErrorAction SilentlyContinue
  $app = Start-Process -FilePath $exe -PassThru
  foreach ($at in 4, 7, 10, 14) { Start-Sleep -Seconds 3; Sample "after start ${at}s" $app }
  (New-Object -ComObject Shell.Application).MinimizeAll()
  Start-Sleep -Seconds 2
  Sample 'desktop in front' $app
  $helper.Show(); $helper.Activate(); [Lab.Win]::Focus($helper.Handle)
  foreach ($at in 2, 5, 9) { Start-Sleep -Seconds 3; Sample "another app ${at}s" $app }
  $where = [Lab.Win]::Widget([uint32]$app.Id)
  if ($where) { $r = $where -split ' ' | ForEach-Object { [int]$_ }; Save-Shot "SHOT$letter" ([Math]::Max(0, $r[0] - 40)) ([Math]::Max(0, $r[1] - 20)) ([Math]::Min($screen.Width, $r[2] - $r[0] + 60)) ([Math]::Min($screen.Height, $r[3] - $r[1] + 40)) }
  else { Save-Shot "SHOT$letter" ([int]($screen.Width / 2)) 0 ([int]($screen.Width / 2)) ([int]($screen.Height * 0.7)) }
  $helper.Hide()
  Get-Content $log -ErrorAction SilentlyContinue | Where-Object { $_ -match 'wallpaper|backdrop|fullscreen' } | Select-Object -Last 4 | ForEach-Object { "LAB   trace: $_" }
  Get-Content $log -ErrorAction SilentlyContinue | Select-Object -Last 3 | ForEach-Object { "LAB   last: $_" }
  if (Test-Path (Join-Path $cfg 'crash.log')) { Get-Content (Join-Path $cfg 'crash.log') | Select-Object -Last 3 | ForEach-Object { "LAB   CRASH: $_" } }
  if ($app.HasExited) { "LAB   THE APP EXITED with code $($app.ExitCode)" }
  Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
}

Run 'A' 'own' '' "the desktop's own picture, widget as installed"
Run 'B' 'black' '' 'plain black desktop, no picture, widget as installed'
Run 'C' 'black' ',"widgetOnTop":true' 'plain black desktop, widget kept above other windows'
Run 'D' 'black' ',"widget":{"ink":"light"},"widgetSet":true,"layout":"custom"' 'plain black desktop, text colour Light'
Run 'E' 'off' '' 'a white picture on record but switched off (accessibility, remote sessions), black behind'
Run 'F' 'blackpic' '' 'a black picture, set the way the Settings app does'
Run 'G' 'clearpng' '' 'a see-through picture with white stored underneath'
"LAB split the desktop: " + [Lab.Win]::Split()
Start-Sleep -Seconds 2
"LAB desktop after the split: " + [Lab.Win]::Stack(0)
Run 'H' 'black' '' 'plain black, the desktop keeps its picture in a window of its own'
Run 'I' 'black' ',"widgetOnTop":true' 'the same, widget kept above other windows'
Get-Process weathadock -ErrorAction SilentlyContinue | Stop-Process -Force
Get-WinEvent -FilterHashtable @{ LogName = 'Application'; ProviderName = 'Application Error' } -MaxEvents 20 -ErrorAction SilentlyContinue |
  Where-Object { $_.Message -match 'weathadock' } | ForEach-Object { "LAB   WINDOWS LOGGED A CRASH: " + (($_.Message -split "`n" | Select-Object -First 6) -join ' ; ') }
"LAB done"
