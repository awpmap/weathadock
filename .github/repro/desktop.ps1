# Runs released WeathaDock builds on a test desktop and reports, every few seconds, where the
# app's windows are among the others and whether anything of them can be seen on the screen.
# Only for finding out why the weather disappears after the update to 1.1.16.
#   -Part desktops       the app as installed, on differently set up desktops and in each of its
#                        three ways of showing the weather
#   -Part update         a PC that ran the other version is updated from inside the app; then the
#                        person opens the widget settings and picks Text color: Light
#   -Part update-docked  the same with the weather docked on the taskbar
#   -Part update-pill    the same with the taskbar pill that stands in while the widget is covered
#   -Part places         other places, among them ones with a weather alert in effect
param([string]$Version = '1.1.16', [string]$Part = 'desktops')
$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
$other = if ($Version -eq '1.1.16') { '1.1.15' } else { '1.1.16' }
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
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, System.Text.StringBuilder text, int max);
[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string title);
[DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr window, uint msg, IntPtr w, IntPtr l, uint flags, uint timeout, out IntPtr result);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern bool SystemParametersInfo(uint action, uint param, string value, uint flags);
[DllImport("user32.dll")] public static extern bool SetSysColors(int count, int[] elements, int[] colors);
[DllImport("user32.dll")] public static extern int GetSysColor(int index);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
[DllImport("user32.dll")] public static extern bool AttachThreadInput(uint from, uint to, bool attach);
[DllImport("user32.dll")] public static extern bool InvalidateRect(IntPtr window, IntPtr rect, bool erase);
[DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
[DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr window, int attribute, out int value, int size);

public static string Class(IntPtr h) { var s = new System.Text.StringBuilder(256); GetClassName(h, s, 256); return s.ToString(); }
static string Name(uint pid) { try { return System.Diagnostics.Process.GetProcessById((int)pid).ProcessName; } catch (Exception) { return "?"; } }

static string Title(IntPtr h) { var s = new System.Text.StringBuilder(80); GetWindowText(h, s, 80); return s.ToString(); }
// Which of the app's windows this is: the desktop widget, the taskbar strip, or an ordinary one.
static string Kind(IntPtr h) {
  string t = Title(h);
  if (t == "WeathaDock taskbar") return "strip";
  if (t == "WeathaDock" && (GetWindowLong(h, -20) & 0x80) != 0) return "widget";
  return "window";
}

// The app's own windows and the desktop's, in the stack of top-level windows (top first, the
// number is the place in the stack), how many large windows of other programs lie above the
// app's, the window in use, and the app's windows that exist but are not shown.
public static string Stack(uint app) {
  var o = new System.Text.StringBuilder(); int i = 0, above = 0; bool seen = false;
  for (IntPtr h = GetTopWindow(IntPtr.Zero); h != IntPtr.Zero; h = GetWindow(h, 2), i++) {
    if (!IsWindowVisible(h)) continue;
    uint pid; GetWindowThreadProcessId(h, out pid);
    string c = Class(h); Rect r; GetWindowRect(h, out r);
    int w = r.Right - r.Left, t = r.Bottom - r.Top, cloak = 0;
    DwmGetWindowAttribute(h, 14, out cloak, 4);
    bool desk = c == "Progman" || c == "WorkerW";
    bool mine = app != 0 && pid == app && w > 40 && t > 20;
    if (!desk && !mine) { if (!seen && cloak == 0 && !IsIconic(h) && w > 200 && t > 150) above++; continue; }
    int ex = GetWindowLong(h, -20);
    if (mine) seen = true;
    o.Append(i + ":" + (mine ? "APP-" + Kind(h) + " ex=" + ex.ToString("X") : c) + " " + w + "x" + t + "@" + r.Left + "," + r.Top + ((ex & 8) != 0 ? " top" : "") + (cloak != 0 ? " cloaked" : "") + (IsIconic(h) ? " min" : "")
      + (FindWindowEx(h, IntPtr.Zero, "SHELLDLL_DefView", null) != IntPtr.Zero ? " +icons" : "") + (FindWindowEx(h, IntPtr.Zero, "WorkerW", null) != IntPtr.Zero ? " +paper" : "") + " | ");
  }
  IntPtr f = GetForegroundWindow(); uint fp; GetWindowThreadProcessId(f, out fp);
  o.Append("large windows above: " + above + " | in use: " + Class(f) + "/" + Name(fp));
  if (app != 0) {
    o.Append(" | not shown:");
    for (IntPtr h = GetTopWindow(IntPtr.Zero); h != IntPtr.Zero; h = GetWindow(h, 2)) {
      uint pid; Rect r; GetWindowThreadProcessId(h, out pid);
      if (pid != app || IsWindowVisible(h)) continue;
      string t = Title(h);
      if (!t.StartsWith("WeathaDock") && !t.StartsWith("Welcome")) continue;
      GetWindowRect(h, out r);
      o.Append(" " + Kind(h) + " " + (r.Right - r.Left) + "x" + (r.Bottom - r.Top) + "@" + r.Left + "," + r.Top);
    }
  }
  return o.ToString();
}

// Where one of the app's windows is on the screen ("left top right bottom"), when it is shown.
public static string Find(uint app, string kind) {
  for (IntPtr h = GetTopWindow(IntPtr.Zero); h != IntPtr.Zero; h = GetWindow(h, 2)) {
    uint pid; Rect r; GetWindowThreadProcessId(h, out pid);
    if (pid == app && IsWindowVisible(h) && GetWindowRect(h, out r) && r.Right - r.Left > 40 && r.Bottom - r.Top > 20 && Kind(h) == kind)
      return r.Left + " " + r.Top + " " + r.Right + " " + r.Bottom;
  }
  return "";
}

// Every window a person could see, top first: what else is on the screen.
public static string Windows() {
  var o = new System.Text.StringBuilder(); int n = 0;
  for (IntPtr h = GetTopWindow(IntPtr.Zero); h != IntPtr.Zero && n < 30; h = GetWindow(h, 2)) {
    if (!IsWindowVisible(h) || IsIconic(h)) continue;
    int cloak = 0; DwmGetWindowAttribute(h, 14, out cloak, 4);
    if (cloak != 0) continue;
    Rect r; GetWindowRect(h, out r); int w = r.Right - r.Left, t = r.Bottom - r.Top;
    if (w < 60 || t < 30) continue;
    uint pid; GetWindowThreadProcessId(h, out pid);
    var title = new System.Text.StringBuilder(80); GetWindowText(h, title, 80);
    o.Append("[" + Class(h) + " '" + title + "' " + Name(pid) + " " + w + "x" + t + "@" + r.Left + "," + r.Top + "] "); n++;
  }
  return o.ToString();
}

// Ask the desktop to put its picture in a window of its own, the way wallpaper changes with a
// fade and live-wallpaper programs do.
public static string Split() {
  IntPtr p = FindWindow("Progman", null), res;
  SendMessageTimeout(p, 0x052C, IntPtr.Zero, IntPtr.Zero, 0, 1000, out res);
  SendMessageTimeout(p, 0x052C, new IntPtr(0xD), new IntPtr(1), 0, 1000, out res);
  return "asked " + p;
}

// Make a window the one in use, the way a click on it does.
public static string Front(IntPtr h) {
  if (h == IntPtr.Zero) return "no such window";
  uint pid, me = GetCurrentThreadId(), fg = GetWindowThreadProcessId(GetForegroundWindow(), out pid);
  if (fg != 0 && fg != me) AttachThreadInput(me, fg, true);
  bool ok = SetForegroundWindow(h);
  if (fg != 0 && fg != me) AttachThreadInput(me, fg, false);
  return ok ? "ok" : "refused";
}
public static IntPtr Desktop() { return FindWindow("Progman", null); }
public static IntPtr Titled(string title) { return FindWindow(null, title); }
public static void Repaint() { InvalidateRect(IntPtr.Zero, IntPtr.Zero, true); }
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
          o += "monitor" + i + " picture=[" + Try(() => d.GetWallpaper(id)) + "]; ";
        }
        o += "position=" + Try(() => d.GetPosition().ToString()) + "; colour=" + Try(() => "0x" + d.GetBackgroundColor().ToString("X6")) + "; status=" + Try(() => d.GetStatus().ToString());
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
"LAB os: $($os.ProductName) $($os.DisplayVersion) build $($os.CurrentBuildNumber).$($os.UBR); app $Version; part $Part"
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
"LAB screen: $($screen.Width)x$($screen.Height); screens: " + [System.Windows.Forms.Screen]::AllScreens.Count
"LAB on screen at the start: " + [Lab.Win]::Windows()
(New-Object -ComObject Shell.Application).MinimizeAll()
Start-Sleep -Seconds 2
"LAB on screen with everything minimized: " + [Lab.Win]::Windows()
"LAB desktop at the start: " + [Lab.Win]::Stack(0)

$exe = Join-Path $env:LOCALAPPDATA 'WeathaDock\weathadock.exe'
$cfg = Join-Path $env:APPDATA 'app.weathadock.desktop'
$local = Join-Path $env:LOCALAPPDATA 'app.weathadock.desktop'
$log = Join-Path $env:TEMP 'lab.log'
$env:WEATHADOCK_LOG = $log
$chicago = @{ zip = '60601'; city = 'Chicago'; state = 'IL'; lat = 41.8858; lon = -87.6181 }

# Pictures for the desktop: plain white and plain black.
$dir = Join-Path $env:TEMP 'lab-papers'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$blackPng = Join-Path $dir 'black.png'
$whitePng = Join-Path $dir 'white.png'
$bmp = New-Object System.Drawing.Bitmap $screen.Width, $screen.Height
$g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::White); $g.Dispose(); $bmp.Save($whitePng); $bmp.Dispose()
$bmp = New-Object System.Drawing.Bitmap $screen.Width, $screen.Height
$g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::Black); $g.Dispose(); $bmp.Save($blackPng); $bmp.Dispose()

$jpeg = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object { $_.MimeType -eq 'image/jpeg' }
$quality = New-Object System.Drawing.Imaging.EncoderParameters 1
$quality.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter ([System.Drawing.Imaging.Encoder]::Quality), 45L

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

# The whole screen at half size, into the log.
function Save-Screen($name) {
  $shot = New-Object System.Drawing.Bitmap $screen.Width, $screen.Height
  $g = [System.Drawing.Graphics]::FromImage($shot); $g.CopyFromScreen(0, 0, 0, 0, $shot.Size); $g.Dispose()
  $small = New-Object System.Drawing.Bitmap ([int]($screen.Width / 2)), ([int]($screen.Height / 2))
  $g = [System.Drawing.Graphics]::FromImage($small); $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $g.DrawImage($shot, 0, 0, $small.Width, $small.Height); $g.Dispose(); $shot.Dispose()
  $file = Join-Path $env:TEMP "$name.jpg"
  $small.Save($file, $jpeg, $quality); $small.Dispose()
  Write-Output "$name-BEGIN"
  [Convert]::ToBase64String([IO.File]::ReadAllBytes($file)) -split '(.{4000})' | Where-Object { $_ }
  Write-Output "$name-END"
}

function Set-Desk($kind) {
  [Lab.Paper]::Enable($true) | Out-Null
  switch ($kind) {
    'black' { [Lab.Win]::SystemParametersInfo(0x0014, 0, '', 3) | Out-Null; [Lab.Win]::SetSysColors(1, @(1), @(0)) | Out-Null }
    'off' { "LAB   a white picture first: " + [Lab.Paper]::Picture($whitePng); Start-Sleep -Seconds 2; "LAB   switch the picture off: " + [Lab.Paper]::Enable($false) + ", colour: " + [Lab.Paper]::Colour(0) }
    'blackpic' { "LAB   picture through the newer way: " + [Lab.Paper]::Picture($blackPng) }
  }
  Start-Sleep -Seconds 2
  [Lab.Win]::Repaint()
  Start-Sleep -Seconds 1
  "LAB   desktop on record: " + [Lab.Paper]::Describe()
}

function Write-Settings($place, $extra) {
  $s = [ordered]@{
    locations = @([ordered]@{ zip = $place.zip; city = $place.city; state = $place.state; lat = $place.lat; lon = $place.lon })
    activeZip = $place.zip
  }
  if ($extra) { foreach ($k in $extra.Keys) { $s[$k] = $extra[$k] } }
  New-Item -ItemType Directory -Force -Path $cfg | Out-Null
  [IO.File]::WriteAllText((Join-Path $cfg 'settings.json'), ($s | ConvertTo-Json -Depth 6))
}

function Install($ver, $switches) {
  $setup = Join-Path $env:TEMP "WeathaDock_${ver}_x64-setup.exe"
  if (-not (Test-Path $setup)) { Invoke-WebRequest "https://github.com/awpmap/weathadock/releases/download/v$ver/WeathaDock_${ver}_x64-setup.exe" -OutFile $setup -TimeoutSec 120 }
  $p = Start-Process $setup -ArgumentList $switches -PassThru
  if (-not $p.WaitForExit(120000)) { "LAB   the $ver installer did not finish in two minutes"; Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
  Start-Sleep -Seconds 2
  "LAB   installed with $($switches -join ' '): " + $(if (Test-Path $exe) { (Get-Item $exe).VersionInfo.ProductVersion } else { 'nothing' })
}

# -Clean also ends what is left of the app's web view, so that the next start is a fresh one.
function Stop-App([switch]$Clean) {
  Get-Process weathadock -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Seconds 1
  if ($Clean) {
    Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" -ErrorAction SilentlyContinue | Where-Object { $_.CommandLine -match 'weathadock' } |
      ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Seconds 2
  }
}
function Start-App { $script:app = Start-Process -FilePath $exe -PassThru }

# A second program whose maximized window covers the whole desktop, the widget included. It
# runs by itself, with its own message loop, like any other app.
$coverScript = Join-Path $env:TEMP 'lab-cover.ps1'
@'
Add-Type -AssemblyName System.Windows.Forms
$f = New-Object Windows.Forms.Form
$f.Text = 'Another app'; $f.WindowState = 'Maximized'; $f.BackColor = 'White'
[Windows.Forms.Application]::Run($f)
'@ | Set-Content $coverScript
function Open-Cover {
  $script:cover = Start-Process pwsh -ArgumentList '-NoProfile', '-File', $coverScript -PassThru
  $h = [IntPtr]::Zero
  foreach ($try in 1..32) { Start-Sleep -Milliseconds 250; $h = [Lab.Win]::Titled('Another app'); if ($h -ne [IntPtr]::Zero) { break } }
  Start-Sleep -Milliseconds 500
  "LAB   another app opens over everything and is the one in use: " + [Lab.Win]::Front($h)
  Start-Sleep -Seconds 3
}
function Close-Cover {
  if ($script:cover) { Stop-Process -Id $script:cover.Id -Force -ErrorAction SilentlyContinue; $script:cover = $null }
  Start-Sleep -Seconds 1
}

# One look: is the app alive, where are its widget and its taskbar strip and how light is the
# screen there (dark text or nothing on black reads 0/0), and the stack of windows.
function Sample($label) {
  $id = [uint32]$script:app.Id
  $seen = foreach ($kind in 'widget', 'strip') {
    $where = [Lab.Win]::Find($id, $kind)
    if ($where) {
      $r = $where -split ' ' | ForEach-Object { [int]$_ }
      '{0}=[{1}] light={2}' -f $kind, $where, (Get-Light $r[0] $r[1] ($r[2] - $r[0]) ($r[3] - $r[1]))
    } else { "$kind=none" }
  }
  "LAB   {0,-34} alive={1} {2} :: {3}" -f $label, (-not $script:app.HasExited), ($seen -join ' '), [Lab.Win]::Stack($id)
}

# What the app has written: its settings, its stored forecast, its own notes and any crash.
function Show-Files {
  $file = Join-Path $cfg 'settings.json'
  if (Test-Path $file) {
    try { $j = Get-Content $file -Raw | ConvertFrom-Json; "LAB   settings on disk: version=$($j.version) ink=$($j.widget.ink) widgetSet=$($j.widgetSet) layout=$($j.layout) glass=$($j.glass) showWidget=$($j.showWidget) onTop=$($j.widgetOnTop) placement=$($j.placement) coveredMode=$($j.coveredMode) widgetPos=$($j.widgetPos.x),$($j.widgetPos.y) pillPos=$($j.pillPos.x),$($j.pillPos.y)" }
    catch { "LAB   settings on disk could not be read: $_" }
  } else { "LAB   no settings file" }
  $file = Join-Path $cfg 'forecast-cache.json'
  if (Test-Path $file) {
    try {
      $j = Get-Content $file -Raw | ConvertFrom-Json
      foreach ($p in $j.PSObject.Properties) { "LAB   stored forecast $($p.Name): status=$($p.Value.status) updatedAt=$($p.Value.updatedAt) utcOffset=$($p.Value.utcOffset) alerts=$(@($p.Value.alerts).Count) temp=$($p.Value.current.temp) condition=$($p.Value.current.condition) hours=$(@($p.Value.hourly).Count) days=$(@($p.Value.daily).Count)" }
    } catch { "LAB   stored forecast could not be read: $_" }
  } else { "LAB   no stored forecast" }
  Get-Content $log -ErrorAction SilentlyContinue | Select-Object -Last 40 | ForEach-Object { "LAB   note: $_" }
  if (Test-Path (Join-Path $cfg 'crash.log')) { Get-Content (Join-Path $cfg 'crash.log') | Select-Object -Last 5 | ForEach-Object { "LAB   CRASH: $_" } }
  if ($script:app.HasExited) { "LAB   THE APP IS NO LONGER RUNNING" }
}

function Click-Desktop {
  "LAB   click on the desktop: " + [Lab.Win]::Front([Lab.Win]::Desktop())
  Start-Sleep -Seconds 2
  Sample 'desktop in use'
}

# One run of the app on one desktop: start it on the bare desktop, click the desktop, cover it
# with another app's window, take the window away again.
function Probe($letter, $desk, $extra, $note) {
  Stop-App
  "LAB $letter ===== $note"
  Set-Desk $desk
  Write-Settings $chicago $extra
  Remove-Item $log -ErrorAction SilentlyContinue
  Remove-Item (Join-Path $cfg 'crash.log') -ErrorAction SilentlyContinue
  Start-App
  foreach ($at in 5, 10, 15) { Start-Sleep -Seconds 5; Sample "started ${at}s" }
  Click-Desktop
  Open-Cover
  Sample 'another app in use 4s'
  Start-Sleep -Seconds 6
  Sample 'another app in use 10s'
  Close-Cover
  Sample 'uncovered 1s'
  Start-Sleep -Seconds 5
  Sample 'uncovered 6s'
  Start-Sleep -Seconds 8
  Sample 'uncovered 14s'
  "LAB   on screen: " + [Lab.Win]::Windows()
  Save-Screen "SHOT$letter"
  Show-Files
}

# The same with the other app's window already open when WeathaDock starts, as after an update
# from inside the app while a browser or the Settings window is in front.
function Probe-Covered($letter, $desk, $extra, $note) {
  Stop-App
  "LAB $letter ===== $note"
  Set-Desk $desk
  Write-Settings $chicago $extra
  Remove-Item $log -ErrorAction SilentlyContinue
  Remove-Item (Join-Path $cfg 'crash.log') -ErrorAction SilentlyContinue
  Open-Cover
  Start-App
  foreach ($at in 5, 12, 20) { Start-Sleep -Seconds $(if ($at -eq 5) { 5 } elseif ($at -eq 12) { 7 } else { 8 }); Sample "started under another app ${at}s" }
  Close-Cover
  Sample 'uncovered 1s'
  Start-Sleep -Seconds 5
  Sample 'uncovered 6s'
  Start-Sleep -Seconds 8
  Sample 'uncovered 14s'
  Click-Desktop
  "LAB   on screen: " + [Lab.Win]::Windows()
  Save-Screen "SHOT$letter"
  Show-Files
}

# The app's own pages, asked through the web view's debugging port (see drive.js). The port is
# only open in the run that needs it.
function Drive { & node (Join-Path $PSScriptRoot 'drive.js') @args 2>&1 | ForEach-Object { "$_" } }

# What the person did after the update: open the widget settings, pick Text color: Light, go
# back to another app, look at the desktop again.
function Probe-Light($letter) {
  Stop-App -Clean
  "LAB $letter ===== the person opens the widget settings and picks Text color: Light"
  Remove-Item $log -ErrorAction SilentlyContinue
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9222'
  Start-App
  Start-Sleep -Seconds 12
  Sample 'started 12s'
  Drive state
  Click-Desktop
  Drive open
  Start-Sleep -Seconds 5
  Sample 'settings window open 5s'
  Drive light
  Start-Sleep -Seconds 2
  Sample 'Light picked 2s'
  Drive state
  Start-Sleep -Seconds 6
  Sample 'Light picked 8s'
  Save-Screen "SHOT${letter}A"
  Drive close
  Start-Sleep -Seconds 2
  Sample 'settings window closed 2s'
  Start-Sleep -Seconds 6
  Sample 'settings window closed 8s'
  Open-Cover
  Sample 'another app in use 4s'
  Start-Sleep -Seconds 8
  Sample 'another app in use 12s'
  Close-Cover
  Sample 'uncovered 1s'
  Click-Desktop
  foreach ($at in 15, 30, 45, 60, 75, 90) { Start-Sleep -Seconds 15; Sample "desktop in use ${at}s"; if ($at -eq 30) { Drive state } }
  "LAB   on screen: " + [Lab.Win]::Windows()
  Save-Screen "SHOT${letter}B"
  Drive state
  Drive shot "PAGE$letter"
  Show-Files
  Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
}

# Places where a weather alert is in effect right now, from the Weather Service's own list.
function Find-AlertPlaces {
  $found = @()
  try {
    $head = @{ 'Accept' = 'application/geo+json' }
    $feed = Invoke-RestMethod 'https://api.weather.gov/alerts/active?status=actual&message_type=alert' -Headers $head -UserAgent 'weathadock-lab' -TimeoutSec 40
    Write-Host ("LAB   alerts in effect in the whole country: " + @($feed.features).Count)
    $seen = @{}
    foreach ($f in $feed.features) {
      if ($found.Count -ge 3) { break }
      try {
        $p = $f.properties
        if ($p.event -notmatch 'Winter|Snow|Ice|Freez|Frost|Cold|Wind|Fog|Flood|Heat|Fire|Red Flag|Blizzard|Advisory') { continue }
        if ($seen[$p.event]) { continue }
        $zone = [string](@($p.affectedZones)[0])
        if ($zone -notmatch '/zones/(forecast|county|fire)/[A-Z]{2}[ZC]\d{3}$' -or $zone -match '/(AM|AN|GM|LC|LE|LH|LM|LO|LS|PH|PK|PM|PS|PZ|SL)Z\d{3}$') { continue }
        $z = Invoke-RestMethod $zone -Headers $head -UserAgent 'weathadock-lab' -TimeoutSec 30
        if (-not $z.geometry) { continue }
        $ring = $z.geometry.coordinates[0]
        if ($z.geometry.type -eq 'MultiPolygon') { $ring = $ring[0] }
        $lon = ($ring | ForEach-Object { $_[0] } | Measure-Object -Average).Average
        $lat = ($ring | ForEach-Object { $_[1] } | Measure-Object -Average).Average
        if (-not $lat -or -not $lon) { continue }
        $seen[$p.event] = $true
        $found += @{ zip = ('0001' + $found.Count); city = [string]$z.properties.name; state = [string]$z.properties.state; lat = [math]::Round($lat, 4); lon = [math]::Round($lon, 4); alert = "$($p.event) ($($p.severity))" }
      } catch { Write-Host "LAB   one alert was skipped: $_" }
    }
  } catch { Write-Host "LAB   the list of alerts could not be used: $_" }
  , $found
}

# One place on the plain black desktop, from an empty forecast store.
function Probe-Place($letter, $place, $seconds) {
  Stop-App
  "LAB $letter ===== $($place.city), $($place.state) ($($place.lat), $($place.lon)) $($place.alert)"
  Remove-Item (Join-Path $cfg 'forecast-cache.json') -ErrorAction SilentlyContinue
  Write-Settings $place @{}
  Remove-Item $log -ErrorAction SilentlyContinue
  Remove-Item (Join-Path $cfg 'crash.log') -ErrorAction SilentlyContinue
  Start-App
  $at = 0
  while ($at -lt $seconds) { $step = if ($at -lt 24) { 8 } else { 15 }; Start-Sleep -Seconds $step; $at += $step; Sample "running ${at}s" }
  Save-Screen "SHOT$letter"
  Show-Files
}

switch -Wildcard ($Part) {
  'desktops' {
    Install $Version @('/P')
    if (-not (Test-Path $exe)) { Write-Error 'The installer did not put the app in place.'; exit 1 }
    Probe 'B' 'black' @{} 'plain black desktop, widget as installed'
    Probe-Covered 'BC' 'black' @{} 'plain black desktop, the app starts under another app'
    Probe 'C' 'black' @{ widgetOnTop = $true } 'plain black desktop, widget kept above other windows'
    Probe 'D' 'black' @{ version = 4; widget = @{ ink = 'light' }; widgetSet = $true; layout = 'custom' } 'plain black desktop, text colour Light chosen'
    Probe 'J' 'black' @{ placement = 'taskbar' } 'plain black desktop, the weather docked on the taskbar'
    Probe 'K' 'black' @{ coveredMode = 'taskbar' } 'plain black desktop, widget with the taskbar pill while covered'
    Probe 'E' 'off' @{} 'a white picture on record but switched off, black shown'
    Probe 'F' 'blackpic' @{} 'a black picture, set the way the Settings app does'
    "LAB split the desktop: " + [Lab.Win]::Split()
    Start-Sleep -Seconds 2
    "LAB desktop after the split: " + [Lab.Win]::Stack(0)
    Probe 'H' 'black' @{} 'plain black, the desktop keeps its picture in a window of its own'
    Probe 'I' 'black' @{ widgetOnTop = $true } 'the same, widget kept above other windows'
  }
  'update*' {
    $how = switch ($Part) { 'update-docked' { @{ placement = 'taskbar' } } 'update-pill' { @{ coveredMode = 'taskbar' } } default { @{} } }
    "LAB U ===== a PC that has run $other is updated to $Version, the way the app updates itself ($Part)"
    Remove-Item $cfg, $local -Recurse -Force -ErrorAction SilentlyContinue
    Set-Desk 'black'
    Install $other @('/P')
    if (-not (Test-Path $exe)) { Write-Error 'The installer did not put the app in place.'; exit 1 }
    Stop-App
    Write-Settings $chicago $how
    Remove-Item $log -ErrorAction SilentlyContinue
    Start-App
    foreach ($at in 15, 30, 45) { Start-Sleep -Seconds 15; Sample "$other running ${at}s" }
    Show-Files
    Save-Screen 'SHOTU0'
    # The update itself. The person has a window open in front (the Settings window they clicked
    # "update" in, a browser): the app quits, the installer runs without questions and starts
    # the new version.
    $setup = Join-Path $env:TEMP "WeathaDock_${Version}_x64-setup.exe"
    Invoke-WebRequest "https://github.com/awpmap/weathadock/releases/download/v$Version/WeathaDock_${Version}_x64-setup.exe" -OutFile $setup -TimeoutSec 120
    Open-Cover
    Remove-Item $log -ErrorAction SilentlyContinue
    "LAB   now the update to ${Version}: installer started with /P /R, app quits"
    $inst = Start-Process $setup -ArgumentList '/P', '/R' -PassThru
    Stop-Process -Id $script:app.Id -Force -ErrorAction SilentlyContinue
    if (-not $inst.WaitForExit(120000)) { "LAB   the installer did not finish in two minutes"; Stop-Process -Id $inst.Id -Force -ErrorAction SilentlyContinue }
    $found = $null
    foreach ($try in 1..20) { $found = Get-Process weathadock -ErrorAction SilentlyContinue | Select-Object -First 1; if ($found) { break }; Start-Sleep -Seconds 1 }
    if ($found) { $script:app = $found; "LAB   the installer started the app: version " + (Get-Item $exe).VersionInfo.ProductVersion + ", processes: " + @(Get-Process weathadock -ErrorAction SilentlyContinue).Count }
    else { "LAB   THE INSTALLER DID NOT START THE APP; starting it by hand"; Start-App }
    foreach ($at in 5, 10, 20) { Start-Sleep -Seconds $(if ($at -eq 20) { 10 } else { 5 }); Sample "$Version under the other app ${at}s" }
    Close-Cover
    Sample 'uncovered 1s'
    foreach ($at in 6, 12, 20, 30) { Start-Sleep -Seconds $(if ($at -eq 6) { 5 } elseif ($at -eq 12) { 6 } elseif ($at -eq 20) { 8 } else { 10 }); Sample "uncovered ${at}s" }
    Save-Screen 'SHOTU1'
    Click-Desktop
    Open-Cover
    Sample 'another app in use 4s'
    Start-Sleep -Seconds 10
    Sample 'another app in use 14s'
    Close-Cover
    Sample 'uncovered again 1s'
    foreach ($at in 15, 30, 45, 60) { Start-Sleep -Seconds 15; Sample "uncovered again ${at}s" }
    "LAB   on screen: " + [Lab.Win]::Windows()
    Save-Screen 'SHOTU2'
    Show-Files
    "LAB U2 ===== the next sign-in: the app is started again"
    Stop-App
    Remove-Item $log -ErrorAction SilentlyContinue
    Start-App
    foreach ($at in 5, 10, 20, 30) { Start-Sleep -Seconds $(if ($at -le 10) { 5 } else { 10 }); Sample "started again ${at}s" }
    Save-Screen 'SHOTU3'
    Show-Files
    Probe-Light 'U4'
  }
  'places' {
    Set-Desk 'black'
    Install $Version @('/P')
    if (-not (Test-Path $exe)) { Write-Error 'The installer did not put the app in place.'; exit 1 }
    $n = 0
    foreach ($place in (Find-AlertPlaces)) { $n++; Probe-Place "A$n" $place 150 }
    if ($n -eq 0) { "LAB   no place with an alert was found" }
    $places = @(
      @{ zip = '10001'; city = 'New York'; state = 'NY'; lat = 40.7506; lon = -73.9972 },
      @{ zip = '80202'; city = 'Denver'; state = 'CO'; lat = 39.7525; lon = -104.9995 },
      @{ zip = '98101'; city = 'Seattle'; state = 'WA'; lat = 47.6101; lon = -122.3344 },
      @{ zip = '33101'; city = 'Miami'; state = 'FL'; lat = 25.7791; lon = -80.1978 },
      @{ zip = '99501'; city = 'Anchorage'; state = 'AK'; lat = 61.2166; lon = -149.8826 },
      @{ zip = '96813'; city = 'Honolulu'; state = 'HI'; lat = 21.3136; lon = -157.8480 }
    )
    $n = 0
    foreach ($place in $places) { $n++; Probe-Place "P$n" $place 24 }
  }
}

Close-Cover
Stop-App
Get-WinEvent -FilterHashtable @{ LogName = 'Application'; ProviderName = 'Application Error' } -MaxEvents 20 -ErrorAction SilentlyContinue |
  Where-Object { $_.Message -match 'weathadock|msedgewebview2' } | ForEach-Object { "LAB   WINDOWS LOGGED A CRASH: " + (($_.Message -split "`n" | Select-Object -First 6) -join ' ; ') }
"LAB done"
exit 0
