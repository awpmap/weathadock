# Part of the temporary desktop lab: the weather docked on the taskbar, and nothing else.
# Runs a released WeathaDock build on a test desktop set up like a dark Windows 11 PC and looks
# at the strip every few seconds while windows open and close, the PC is busy, a full-screen
# window comes and goes, the taskbar hides itself and the screen gets larger. For every stretch
# it counts how often the strip was where it belongs and drawn. The counts go into a short
# summary file, so that one job can print the summaries of all runs together.
# -Installer: a setup file to install instead of the released one (the app built from a branch).
param([string]$Version = '1.1.16', [string]$Installer = '')
$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -Namespace Lab -Name W -MemberDefinition @'
public struct Rect { public int Left, Top, Right, Bottom; }
[StructLayout(LayoutKind.Sequential)] public struct BarData { public int size; public IntPtr window; public uint message; public uint edge; public Rect rect; public IntPtr param; }
[DllImport("user32.dll")] public static extern IntPtr GetTopWindow(IntPtr parent);
[DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr window, uint cmd);
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, System.Text.StringBuilder text, int max);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr window, System.Text.StringBuilder text, int max);
[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
[DllImport("user32.dll")] public static extern bool AttachThreadInput(uint from, uint to, bool attach);
[DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
[DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
[DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extra);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string title);
// The open menu: "left top right bottom", then what lies over it ("nothing", "the strip",
// "the taskbar"), or "none".
public static string Menu(uint app) {
  IntPtr menu = IntPtr.Zero;
  for (IntPtr h = FindWindowEx(IntPtr.Zero, IntPtr.Zero, "#32768", null); h != IntPtr.Zero; h = FindWindowEx(IntPtr.Zero, h, "#32768", null))
    if (IsWindowVisible(h)) { menu = h; break; }
  if (menu == IntPtr.Zero) return "none";
  Rect m; GetWindowRect(menu, out m);
  IntPtr bar = FindWindow("Shell_TrayWnd", null);
  string over = "nothing";
  for (IntPtr h = GetTopWindow(IntPtr.Zero); h != IntPtr.Zero && h != menu; h = GetWindow(h, 2)) {
    if (!IsWindowVisible(h)) continue;
    Rect r; GetWindowRect(h, out r);
    if (r.Right <= m.Left || r.Left >= m.Right || r.Bottom <= m.Top || r.Top >= m.Bottom) continue;
    if (h == bar) { over = "the taskbar"; break; }
    uint pid; GetWindowThreadProcessId(h, out pid);
    var s = new System.Text.StringBuilder(80); GetWindowText(h, s, 80);
    if (pid == app && s.ToString() == "WeathaDock taskbar") { over = "the strip"; break; }
  }
  return m.Left + " " + m.Top + " " + m.Right + " " + m.Bottom + " " + over;
}
[DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern bool SystemParametersInfo(uint action, uint param, string value, uint flags);
[DllImport("user32.dll")] public static extern bool SetSysColors(int count, int[] elements, int[] colors);
[DllImport("user32.dll", CharSet = CharSet.Unicode, EntryPoint = "SendMessageTimeoutW")] static extern IntPtr SendNotice(IntPtr window, uint msg, IntPtr w, string l, uint flags, uint timeout, out IntPtr result);
[DllImport("shell32.dll")] static extern UIntPtr SHAppBarMessage(uint msg, ref BarData data);

// The app's taskbar strip: "shown left top right bottom above|below" (above or below the
// taskbar in the stack of windows), "hidden left top right bottom", or "none".
public static string Strip(uint app) {
  IntPtr bar = FindWindow("Shell_TrayWnd", null); bool barSeen = false; string hidden = "none";
  for (IntPtr h = GetTopWindow(IntPtr.Zero); h != IntPtr.Zero; h = GetWindow(h, 2)) {
    if (h == bar) { barSeen = true; continue; }
    uint pid; GetWindowThreadProcessId(h, out pid);
    if (pid != app) continue;
    var s = new System.Text.StringBuilder(80); GetWindowText(h, s, 80);
    if (s.ToString() != "WeathaDock taskbar") continue;
    Rect r; GetWindowRect(h, out r);
    string where = r.Left + " " + r.Top + " " + r.Right + " " + r.Bottom;
    if (IsWindowVisible(h)) return "shown " + where + (barSeen ? " below" : " above");
    hidden = "hidden " + where;
  }
  return hidden;
}
public static string Bar() {
  Rect r; IntPtr bar = FindWindow("Shell_TrayWnd", null);
  if (bar == IntPtr.Zero || !GetWindowRect(bar, out r)) return "none";
  return r.Left + " " + r.Top + " " + r.Right + " " + r.Bottom;
}
public static string Fore() { var s = new System.Text.StringBuilder(256); GetClassName(GetForegroundWindow(), s, 256); return s.ToString(); }
// Make a window the one in use, the way a click on it does.
public static string Front(IntPtr h) {
  if (h == IntPtr.Zero) return "no such window";
  uint pid, me = GetCurrentThreadId(), fg = GetWindowThreadProcessId(GetForegroundWindow(), out pid);
  if (fg != 0 && fg != me) AttachThreadInput(me, fg, true);
  bool ok = SetForegroundWindow(h);
  if (fg != 0 && fg != me) AttachThreadInput(me, fg, false);
  return ok ? "ok" : "refused";
}
public static IntPtr Titled(string title) { return FindWindow(null, title); }
public static IntPtr Desktop() { return FindWindow("Progman", null); }
// Tell every program that the light or dark setting has changed, as the Settings app does.
public static void ThemeChanged() { IntPtr r; SendNotice(new IntPtr(0xFFFF), 0x001A, IntPtr.Zero, "ImmersiveColorSet", 2, 3000, out r); }
// "Automatically hide the taskbar", on or off.
public static string HideBar(bool on) {
  var d = new BarData(); d.size = Marshal.SizeOf(typeof(BarData)); d.window = FindWindow("Shell_TrayWnd", null);
  d.param = new IntPtr(on ? 1 : 2);
  SHAppBarMessage(10, ref d);
  return "state " + SHAppBarMessage(4, ref d);
}
'@
# Another screen size. Kept apart, so that a mistake here cannot take the helpers above with it.
try {
  Add-Type -Namespace Lab -Name Screen -MemberDefinition @'
[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)] public struct Mode {
  [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string device;
  public short specVersion, driverVersion, size, driverExtra; public int fields;
  public int positionX, positionY, orientation, fixedOutput;
  public short color, duplex, yResolution, ttOption, collate;
  [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string form;
  public short logPixels; public int bitsPerPel, width, height, flags, frequency;
  public int icmMethod, icmIntent, mediaType, ditherType, reserved1, reserved2, panningWidth, panningHeight;
}
[DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern bool EnumDisplaySettings(string device, int mode, ref Mode m);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int ChangeDisplaySettings(ref Mode m, int flags);
public static string Resize(int width, int height) {
  var m = new Mode(); m.size = (short)Marshal.SizeOf(typeof(Mode));
  if (!EnumDisplaySettings(null, -1, ref m)) return "the display mode could not be read";
  string was = m.width + "x" + m.height;
  m.width = width; m.height = height; m.fields = 0x80000 | 0x100000;
  return "was " + was + ", answer " + ChangeDisplaySettings(ref m, 0);
}
'@
} catch { "LAB   the screen size helper did not build: $_" }

$summary = New-Object System.Collections.ArrayList
function Note($text) { [void]$summary.Add($text); "LAB SUM $text" }
function Write-Summary { if ($env:RUNNER_TEMP) { [IO.File]::WriteAllLines((Join-Path $env:RUNNER_TEMP 'lab-summary.txt'), [string[]]$summary) } }

$os = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
Note "app $Version on $($os.ProductName) $($os.DisplayVersion) build $($os.CurrentBuildNumber).$($os.UBR) ($env:PROCESSOR_ARCHITECTURE), screen $([Lab.W]::GetSystemMetrics(0))x$([Lab.W]::GetSystemMetrics(1))"
(New-Object -ComObject Shell.Application).MinimizeAll()
Start-Sleep -Seconds 2

$exe = Join-Path $env:LOCALAPPDATA 'WeathaDock\weathadock.exe'
$cfg = Join-Path $env:APPDATA 'app.weathadock.desktop'
$log = Join-Path $env:TEMP 'lab.log'
$env:WEATHADOCK_LOG = $log
$began = Get-Date

$jpeg = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object { $_.MimeType -eq 'image/jpeg' }
$quality = New-Object System.Drawing.Imaging.EncoderParameters 1
$quality.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter ([System.Drawing.Imaging.Encoder]::Quality), 55L

# The darkest and the lightest spot (0 to 255) of a part of the screen, on a small copy. Text
# drawn there sets the two far apart; an empty stretch of taskbar leaves them together.
function Get-Contrast([int]$left, [int]$top, [int]$wide, [int]$high) {
  if ($wide -lt 8 -or $high -lt 8) { return '0 0' }
  try {
    $shot = New-Object System.Drawing.Bitmap $wide, $high
    $g = [System.Drawing.Graphics]::FromImage($shot); $g.CopyFromScreen($left, $top, 0, 0, $shot.Size); $g.Dispose()
    $sw = [Math]::Max(16, [Math]::Min(200, [int]($wide / 3))); $sh = 16
    $small = New-Object System.Drawing.Bitmap $sw, $sh
    $g = [System.Drawing.Graphics]::FromImage($small); $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBilinear
    $g.DrawImage($shot, 0, 0, $sw, $sh); $g.Dispose()
    $data = $small.LockBits((New-Object System.Drawing.Rectangle 0, 0, $sw, $sh), [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $bytes = New-Object byte[] ($data.Stride * $sh)
    [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $bytes.Length)
    $small.UnlockBits($data); $small.Dispose(); $shot.Dispose()
    $min = 255; $max = 0
    for ($i = 1; $i -lt $bytes.Length; $i += 4) { $v = $bytes[$i]; if ($v -gt $max) { $max = $v }; if ($v -lt $min) { $min = $v } }
    "$min $max"
  } catch { '0 0' }
}

# The bottom of the screen, where the taskbar is, into the log.
function Save-Bar($name, [int]$high = 96) {
  try {
    $w = [Lab.W]::GetSystemMetrics(0); $h = [Lab.W]::GetSystemMetrics(1)
    $shot = New-Object System.Drawing.Bitmap $w, $high
    $g = [System.Drawing.Graphics]::FromImage($shot); $g.CopyFromScreen(0, $h - $high, 0, 0, $shot.Size); $g.Dispose()
    $file = Join-Path $env:TEMP "$name.jpg"
    $shot.Save($file, $jpeg, $quality); $shot.Dispose()
    "$name-BEGIN"
    [Convert]::ToBase64String([IO.File]::ReadAllBytes($file)) -split '(.{4000})' | Where-Object { $_ }
    "$name-END"
  } catch { "LAB   no picture ${name}: $_" }
}

function Install($ver) {
  $setup = $Installer
  if (-not $setup) {
    $setup = Join-Path $env:TEMP "WeathaDock_${ver}_x64-setup.exe"
    Invoke-WebRequest "https://github.com/awpmap/weathadock/releases/download/v$ver/WeathaDock_${ver}_x64-setup.exe" -OutFile $setup -TimeoutSec 180
  }
  $p = Start-Process $setup -ArgumentList '/P' -PassThru
  if (-not $p.WaitForExit(180000)) { "LAB   the installer did not finish in three minutes"; Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
  Start-Sleep -Seconds 2
  "LAB   installed: " + $(if (Test-Path $exe) { (Get-Item $exe).VersionInfo.ProductVersion } else { 'nothing' })
}
function Stop-App { Get-Process weathadock -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Seconds 1 }

# One look at the strip. Where it belongs is the left half of the taskbar, drawn.
$stats = [ordered]@{}
function Look($phase) {
  $alive = -not $script:app.HasExited
  $bar = [Lab.W]::Bar()
  $b = $bar -split ' '
  $p = [Lab.W]::Strip([uint32]$script:app.Id) -split ' '
  $state = $p[0]; $light = '-'; $drawn = $false; $below = $false
  if ($state -eq 'shown') {
    $l = [int]$p[1]; $t = [int]$p[2]; $w = [int]$p[3] - $l; $h = [int]$p[4] - $t
    $below = $p[5] -eq 'below'
    if ($t -ge [Lab.W]::GetSystemMetrics(1) - 4) { $state = 'off-screen' }
    else {
      $light = Get-Contrast $l $t $w $h
      $m = $light -split ' '; $drawn = ([int]$m[1] - [int]$m[0]) -ge 60
      $mid = if ($b.Count -eq 4) { ([int]$b[0] + [int]$b[2]) / 2 } else { [Lab.W]::GetSystemMetrics(0) / 2 }
      $state = if (($l + $w / 2) -lt $mid) { 'left' } else { 'right' }
    }
  }
  if (-not $script:stats.Contains($phase)) { $script:stats[$phase] = @{ looks = 0; alive = 0; good = 0; left = 0; right = 0; hidden = 0; off = 0; none = 0; below = 0; gone = (New-Object System.Collections.ArrayList) } }
  $s = $script:stats[$phase]
  $s.looks++
  if ($alive) { $s.alive++ }
  switch ($state) { 'left' { $s.left++ } 'right' { $s.right++ } 'hidden' { $s.hidden++ } 'off-screen' { $s.off++ } default { $s.none++ } }
  if ($below) { $s.below++ }
  $at = [int]((Get-Date) - $script:began).TotalSeconds
  if ($state -eq 'left' -and $drawn) { $s.good++ }
  elseif ($s.gone.Count -lt 14) { [void]$s.gone.Add("${at}s " + $state + $(if ($state -in 'left', 'right' -and -not $drawn) { ' blank' } else { '' }) + $(if ($below) { ' below' } else { '' })) }
  "LAB   {0,4}s {1,-22} alive={2} strip={3} [{4}] light={5} drawn={6}{7} bar=[{8}] in use={9}" -f $at, $phase, $alive, $state, (($p | Select-Object -Skip 1) -join ' '), $light, $drawn, $(if ($below) { ' BELOW THE TASKBAR' } else { '' }), $bar, [Lab.W]::Fore()
}
function Watch($phase, $seconds, $every) {
  $end = (Get-Date).AddSeconds($seconds)
  while ((Get-Date) -lt $end) { Look $phase; Start-Sleep -Seconds $every }
}

# Helpers that run by themselves, each with its own message loop, like any other program.
$churnScript = Join-Path $env:TEMP 'lab-churn.ps1'
@'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$open = New-Object System.Collections.ArrayList
$script:n = 0
$timer = New-Object Windows.Forms.Timer
$timer.Interval = 4000
$timer.Add_Tick({
  $script:n++
  if ($open.Count -ge 3 -or ($open.Count -gt 0 -and $script:n % 3 -eq 0)) { $f = $open[0]; $open.RemoveAt(0); $f.Close() }
  else {
    $f = New-Object Windows.Forms.Form
    $f.Text = "Lab window $script:n"; $f.StartPosition = 'Manual'
    $f.Location = New-Object Drawing.Point (80 + 60 * $open.Count), (60 + 40 * $open.Count)
    $f.Size = New-Object Drawing.Size 360, 240
    $f.Show(); [void]$open.Add($f)
  }
})
$timer.Start()
[Windows.Forms.Application]::Run()
'@ | Set-Content $churnScript
$fullScript = Join-Path $env:TEMP 'lab-full.ps1'
@'
Add-Type -AssemblyName System.Windows.Forms
$f = New-Object Windows.Forms.Form
$f.Text = 'Lab full screen'; $f.FormBorderStyle = 'None'; $f.StartPosition = 'Manual'
$f.Bounds = [Windows.Forms.Screen]::PrimaryScreen.Bounds; $f.TopMost = $true; $f.BackColor = 'Black'
$t = New-Object Windows.Forms.Timer
$t.Interval = 14000
$t.Add_Tick({ $f.Close() })
$t.Start()
[Windows.Forms.Application]::Run($f)
'@ | Set-Content $fullScript

"LAB ===== a dark Windows with a black desktop, the weather docked on the taskbar"
$theme = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'
if (-not (Test-Path $theme)) { New-Item $theme -Force | Out-Null }
Set-ItemProperty $theme SystemUsesLightTheme 0 -Type DWord
Set-ItemProperty $theme AppsUseLightTheme 0 -Type DWord
[Lab.W]::ThemeChanged()
[Lab.W]::SystemParametersInfo(0x0014, 0, '', 3) | Out-Null
[Lab.W]::SetSysColors(1, @(1), @(0)) | Out-Null
Start-Sleep -Seconds 3

Install $Version
if (-not (Test-Path $exe)) { Note 'THE INSTALLER DID NOT PUT THE APP IN PLACE'; Write-Summary; exit 1 }
Stop-App
$settings = [ordered]@{
  locations = @([ordered]@{ zip = '60601'; city = 'Chicago'; state = 'IL'; lat = 41.8858; lon = -87.6181 })
  activeZip = '60601'
  placement = 'taskbar'
  taskbar = @{ hours = 6 }
}
New-Item -ItemType Directory -Force -Path $cfg | Out-Null
[IO.File]::WriteAllText((Join-Path $cfg 'settings.json'), ($settings | ConvertTo-Json -Depth 6))
Remove-Item $log -ErrorAction SilentlyContinue
Remove-Item (Join-Path $cfg 'crash.log') -ErrorAction SilentlyContinue
$app = Start-Process -FilePath $exe -PassThru
Start-Sleep -Seconds 12
$b = [Lab.W]::Bar() -split ' '
if ($b.Count -eq 4) { "LAB   the taskbar is this light at its far left: " + (Get-Contrast ([int]$b[0] + 2) ([int]$b[1] + 4) 8 40) }
Save-Bar 'BAR1'
Watch 'quiet' 45 3

"LAB ===== windows open and close every four seconds, and a second program keeps asking the taskbar where its buttons are"
$probeOut = Join-Path $env:TEMP 'lab-probe.txt'
$env:LAB_PROBE_SECONDS = '75'
$probeScript = Join-Path $PSScriptRoot 'probe.ps1'
# The way a person runs it: the text of the script, handed to the Windows PowerShell every PC
# has (started with its own module path, not the one of the PowerShell this lab runs in).
$probeRun = Join-Path $env:TEMP 'lab-probe-run.ps1'
"Invoke-Expression (Get-Content -Raw '$probeScript')" | Set-Content $probeRun
$probe = Start-Process cmd -ArgumentList "/c set PSModulePath=& powershell -NoProfile -ExecutionPolicy Bypass -File $probeRun > $probeOut 2>&1" -NoNewWindow -PassThru
$churn = Start-Process pwsh -ArgumentList '-NoProfile', '-File', $churnScript -PassThru
Watch 'windows come and go' 110 2
Save-Bar 'BAR2'

"LAB ===== the same while every processor is kept busy"
$cores = [Math]::Max(2, [Environment]::ProcessorCount)
$busy = foreach ($n in 1..$cores) { Start-Process pwsh -ArgumentList '-NoProfile', '-Command', '$e = (Get-Date).AddSeconds(45); while ((Get-Date) -lt $e) { }' -NoNewWindow -PassThru }
Watch 'PC kept busy' 45 3
$busy | ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
Stop-Process -Id $churn.Id -Force -ErrorAction SilentlyContinue
Watch 'quiet again' 36 3

"LAB ===== the strip's right-click menu"
$p = [Lab.W]::Strip([uint32]$app.Id) -split ' '
if ($p[0] -eq 'shown') {
  [Lab.W]::SetCursorPos([int](([int]$p[1] + [int]$p[3]) / 2), [int](([int]$p[2] + [int]$p[4]) / 2)) | Out-Null
  Start-Sleep -Milliseconds 500
  [Lab.W]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero); [Lab.W]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Seconds 1
  $first = [Lab.W]::Menu([uint32]$app.Id)
  Start-Sleep -Seconds 3
  $later = [Lab.W]::Menu([uint32]$app.Id)
  Note ("menu right after the right-click: $first; three seconds later: $later; taskbar " + [Lab.W]::Bar())
  Save-Bar 'MENU' 420
  [Lab.W]::SetCursorPos([int]([Lab.W]::GetSystemMetrics(0) / 2), 150) | Out-Null
  Start-Sleep -Milliseconds 300
  [Lab.W]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero); [Lab.W]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Seconds 1
  Note ("menu after a click elsewhere: " + [Lab.W]::Menu([uint32]$app.Id))
} else { Note "no menu check: the strip is $($p -join ' ')" }

"LAB ===== a window takes the whole screen for a quarter of a minute"
$full = Start-Process pwsh -ArgumentList '-NoProfile', '-File', $fullScript -PassThru
$h = [IntPtr]::Zero
foreach ($try in 1..40) { Start-Sleep -Milliseconds 250; $h = [Lab.W]::Titled('Lab full screen'); if ($h -ne [IntPtr]::Zero) { break } }
"LAB   the full-screen window is the one in use: " + [Lab.W]::Front($h)
Start-Sleep -Seconds 2
Watch 'full screen up' 6 2
if (-not $full.WaitForExit(20000)) { Stop-Process -Id $full.Id -Force -ErrorAction SilentlyContinue }
"LAB   the full-screen window is gone; desktop in use: " + [Lab.W]::Front([Lab.W]::Desktop())
Watch 'full screen gone' 42 2
Save-Bar 'BAR3'

"LAB ===== the taskbar is set to hide itself"
$midX = [int]([Lab.W]::GetSystemMetrics(0) / 2); $bottom = [Lab.W]::GetSystemMetrics(1) - 1
"LAB   hide automatically: " + [Lab.W]::HideBar($true)
[Lab.W]::SetCursorPos($midX, 200) | Out-Null
Watch 'taskbar hides itself' 42 3
[Lab.W]::SetCursorPos($midX, $bottom) | Out-Null
# A real movement onto the bottom edge, the way a mouse does it.
[Lab.W]::mouse_event(0x8001, 32768, 65535, 0, [UIntPtr]::Zero)
Watch 'taskbar called up' 12 3
Save-Bar 'BAR4'
[Lab.W]::SetCursorPos($midX, 200) | Out-Null
Watch 'taskbar hidden again' 39 3
"LAB   always shown again: " + [Lab.W]::HideBar($false)
Watch 'taskbar shown again' 21 3

"LAB ===== a larger screen"
try { "LAB   1920 by 1080: " + [Lab.Screen]::Resize(1920, 1080) } catch { "LAB   the screen size could not be changed: $_" }
Start-Sleep -Seconds 6
"LAB   screen now $([Lab.W]::GetSystemMetrics(0))x$([Lab.W]::GetSystemMetrics(1)), taskbar [$([Lab.W]::Bar())]"
Watch 'larger screen' 45 3
Save-Bar 'BAR5'

# What the app noted, and the counts.
Get-Content $log -ErrorAction SilentlyContinue | Select-Object -Last 30 | ForEach-Object { "LAB   note: $_" }
foreach ($phase in $stats.Keys) {
  $s = $stats[$phase]
  Note ("{0,-22} looks={1,3} alive={2,3} in place and drawn={3,3} | left={4} right={5} hidden={6} off-screen={7} no window={8} below the taskbar={9}{10}" -f $phase, $s.looks, $s.alive, $s.good, $s.left, $s.right, $s.hidden, $s.off, $s.none, $s.below, $(if ($s.gone.Count) { ' | not there: ' + ($s.gone -join ', ') } else { '' }))
}
$lines = @(Get-Content $log -ErrorAction SilentlyContinue)
$geo = foreach ($line in $lines) {
  if ($line -match 'pill_geometry docked=(\w+) dock=\((\d+), (\w+)\) spot=(\w+)(?:.*crowded=(\w+))?') { "room=$($Matches[2]) spot=$($Matches[4])" + $(if ($Matches[5]) { " crowded=$($Matches[5])" } else { '' }) }
}
$groups = @($geo | Group-Object | Sort-Object Count -Descending | ForEach-Object { "$($_.Count) x [$($_.Name)]" })
Note ("the app measured the taskbar $(@($geo).Count) times: " + ($groups -join ', '))
foreach ($what in 'out of room -> true', 'out of room -> false', 'tray icon -> temperature', 'still following') { Note ("the app noted '$what' " + @($lines | Where-Object { $_ -like "*$what*" }).Count + ' times') }
if (Test-Path (Join-Path $cfg 'crash.log')) { Get-Content (Join-Path $cfg 'crash.log') | Select-Object -Last 5 | ForEach-Object { Note "CRASH: $_" } }
if ($app.HasExited) { Note 'THE APP IS NO LONGER RUNNING' }
if (-not $probe.WaitForExit(5000)) { Stop-Process -Id $probe.Id -Force -ErrorAction SilentlyContinue }
Get-Content $probeOut -ErrorAction SilentlyContinue | ForEach-Object { Note "asked from outside: $_" }
Stop-App
Get-WinEvent -FilterHashtable @{ LogName = 'Application'; ProviderName = 'Application Error' } -MaxEvents 20 -ErrorAction SilentlyContinue |
  Where-Object { $_.Message -match 'weathadock|msedgewebview2' } | ForEach-Object { Note ("WINDOWS LOGGED A CRASH: " + (($_.Message -split "`n" | Select-Object -First 4) -join ' ; ')) }
Write-Summary
"LAB done"
exit 0
