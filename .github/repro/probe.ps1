# Part of the temporary desktop lab. For half a minute it looks at the docked weather strip the
# way a person does (is it there, where, above or below the taskbar), asks the taskbar where its
# Start button is, the same question WeathaDock asks before it places the strip, and notes the
# few Windows settings that matter for the strip. Written for the Windows PowerShell every PC
# has. Nothing in the result names the person or the place: it is a short text, shown and copied
# to the clipboard.
$seconds = 30
if ($env:LAB_PROBE_SECONDS) { $seconds = [int]$env:LAB_PROBE_SECONDS }
$out = New-Object System.Collections.ArrayList
function Say($text) { [void]$out.Add($text) }
function Reg($path, $name) { try { (Get-ItemProperty -Path $path -Name $name -ErrorAction Stop).$name } catch { 'not set' } }
function Tally($table, $title) {
  Say $title
  foreach ($k in ($table.Keys | Sort-Object { - $table[$_] })) { Say ("  " + $table[$k] + " x " + $k) }
}

$os = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
Say "Windows $($os.DisplayVersion) build $($os.CurrentBuildNumber).$($os.UBR)"
Add-Type -AssemblyName System.Windows.Forms
foreach ($s in [System.Windows.Forms.Screen]::AllScreens) {
  Say "screen $($s.Bounds.Width)x$($s.Bounds.Height) at $($s.Bounds.X),$($s.Bounds.Y), room for windows $($s.WorkingArea.Width)x$($s.WorkingArea.Height), main=$($s.Primary)"
}
$adv = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced'
$hides = 'unknown'
try { $hides = ((Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StuckRects3' -ErrorAction Stop).Settings[8] -band 1) -eq 1 } catch { }
Say ("taskbar: alignment=" + (Reg $adv 'TaskbarAl') + " widgets=" + (Reg $adv 'TaskbarDa') + " combine=" + (Reg $adv 'TaskbarGlomLevel') + " on every screen=" + (Reg $adv 'MMTaskbarEnabled') +
  " hides itself=$hides light=" + (Reg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' 'SystemUsesLightTheme') + " see-through=" + (Reg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' 'EnableTransparency') +
  " text size=" + (Reg 'HKCU:\Control Panel\Desktop\WindowMetrics' 'AppliedDPI'))
# Programs that restyle or move the taskbar, by name only.
$known = 'TranslucentTB', 'windhawk', 'StartAllBackX64', 'StartIsBack64', 'Start11Srv', 'Start11_64', 'TaskbarX', 'RoundedTB', 'SmartTaskbar', 'ExplorerPatcher', 'ep_dwm', 'Rainmeter', 'wallpaper32', 'wallpaper64', 'Lively', 'AutoDarkModeSvc', 'MicaForEveryone', 'DisplayFusion', 'YASB', 'Zebar'
$tools = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $known -contains $_.ProcessName } | ForEach-Object { $_.ProcessName } | Sort-Object -Unique)
Say ("taskbar and desktop tools running: " + $(if ($tools.Count) { $tools -join ', ' } else { 'none of the ones I know' }))

$windows = $false
try {
  Add-Type -Namespace Lab -Name P -ErrorAction Stop -MemberDefinition @'
public struct Rect { public int Left, Top, Right, Bottom; }
[StructLayout(LayoutKind.Sequential)] struct ScreenInfo { public int size; public Rect screen; public Rect work; public uint flags; }
[DllImport("user32.dll")] static extern IntPtr GetTopWindow(IntPtr parent);
[DllImport("user32.dll")] static extern IntPtr GetWindow(IntPtr window, uint cmd);
[DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
[DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window, out Rect rect);
[DllImport("user32.dll", EntryPoint = "GetWindowLongW")] static extern int GetStyle(IntPtr window, int index);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr window, System.Text.StringBuilder text, int max);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr window, System.Text.StringBuilder text, int max);
[DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr FindWindow(string cls, string title);
[DllImport("user32.dll")] static extern IntPtr MonitorFromWindow(IntPtr window, uint flags);
[DllImport("user32.dll", EntryPoint = "GetMonitorInfoW")] static extern bool GetScreenInfo(IntPtr monitor, ref ScreenInfo info);
[DllImport("shell32.dll")] static extern int SHQueryUserNotificationState(out int state);
static string Place(Rect r) { return r.Left + "," + r.Top + " size " + (r.Right - r.Left) + "x" + (r.Bottom - r.Top); }
// The weather strip: hidden, or shown with its place and whether it is above or below the
// taskbar in the stack of windows.
public static string Strip() {
  IntPtr bar = FindWindow("Shell_TrayWnd", null); bool barSeen = false; string found = "no strip window";
  Rect b = new Rect(); if (bar != IntPtr.Zero) GetWindowRect(bar, out b);
  int guard = 0;
  for (IntPtr h = GetTopWindow(IntPtr.Zero); h != IntPtr.Zero && guard < 5000; h = GetWindow(h, 2)) {
    guard++;
    if (h == bar) { barSeen = true; continue; }
    System.Text.StringBuilder s = new System.Text.StringBuilder(80); GetWindowText(h, s, 80);
    if (s.ToString() != "WeathaDock taskbar") continue;
    Rect r; GetWindowRect(h, out r);
    if (!IsWindowVisible(h)) { found = "strip hidden, last at " + Place(r); continue; }
    return "strip shown at " + Place(r) + (barSeen ? ", BELOW the taskbar" : ", above the taskbar") + "; taskbar at " + Place(b);
  }
  return found + "; taskbar at " + Place(b);
}
// The window in use: its kind, whether it fills its screen, and whether Windows holds back
// notifications (2, 3 and 4 mean a full-screen app or a presentation).
public static string Front() {
  IntPtr f = GetForegroundWindow(); int q; SHQueryUserNotificationState(out q);
  if (f == IntPtr.Zero) return "nothing in use, notifications " + q;
  System.Text.StringBuilder c = new System.Text.StringBuilder(256); GetClassName(f, c, 256);
  Rect r; GetWindowRect(f, out r);
  ScreenInfo m = new ScreenInfo(); m.size = Marshal.SizeOf(typeof(ScreenInfo));
  bool fills = GetScreenInfo(MonitorFromWindow(f, 2), ref m) && r.Left <= m.screen.Left && r.Top <= m.screen.Top && r.Right >= m.screen.Right && r.Bottom >= m.screen.Bottom;
  bool titled = (GetStyle(f, -16) & 0x00C00000) == 0x00C00000;
  return c.ToString() + (fills ? (titled ? ", fills its screen with a title bar" : ", fills its screen without a title bar") : "") + ", notifications " + q;
}
'@
  $windows = $true
} catch { Say "the window check did not build ($($_.Exception.Message))" }

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$AE = [System.Windows.Automation.AutomationElement]
$scope = [System.Windows.Automation.TreeScope]
$isBar = New-Object System.Windows.Automation.PropertyCondition($AE::ClassNameProperty, 'Shell_TrayWnd')
$isStart = New-Object System.Windows.Automation.PropertyCondition($AE::AutomationIdProperty, 'StartButton')
$seen = @{}; $strip = @{}; $front = @{}; $count = 0; $slowest = 0; $total = 0
$watch = [System.Diagnostics.Stopwatch]::StartNew()
while ($watch.Elapsed.TotalSeconds -lt $seconds) {
  if ($windows) {
    try { $a = [Lab.P]::Strip(); $strip[$a] = 1 + [int]$strip[$a]; $f = [Lab.P]::Front(); $front[$f] = 1 + [int]$front[$f] } catch { $strip['the look failed'] = 1 + [int]$strip['the look failed'] }
  }
  $t = [System.Diagnostics.Stopwatch]::StartNew()
  $what = ''
  try {
    $bar = $AE::RootElement.FindFirst($scope::Children, $isBar)
    if (-not $bar) { $what = 'no taskbar found' }
    else {
      $b = $bar.Current.BoundingRectangle
      $start = $bar.FindFirst($scope::Descendants, $isStart)
      if (-not $start) { $what = 'no Start button found' }
      else {
        $r = $start.Current.BoundingRectangle
        if ($r.IsEmpty -or $r.Width -le 0) { $what = 'a Start button without a place' }
        else { $what = "Start button at $([int]$r.Left), $([int]$r.Width) wide, taskbar $([int]$b.Left) to $([int]$b.Right)" }
      }
    }
  } catch { $what = 'the question failed (' + $_.Exception.GetType().Name + ')' }
  $ms = [int]$t.ElapsedMilliseconds
  $total += $ms
  if ($ms -gt $slowest) { $slowest = $ms }
  $count++
  $seen[$what] = 1 + [int]$seen[$what]
  Start-Sleep -Milliseconds 250
}
if ($windows) {
  Tally $strip "looked at the weather strip $count times in $seconds seconds:"
  Tally $front 'the window in use at those times:'
}
Tally $seen "asked the taskbar $count times (an answer took $([int]($total / [Math]::Max(1, $count))) ms on average, $slowest ms at the slowest):"

$exe = Join-Path $env:LOCALAPPDATA 'WeathaDock\weathadock.exe'
if (Test-Path $exe) { Say ("WeathaDock " + (Get-Item $exe).VersionInfo.ProductVersion + ", running: " + [bool](Get-Process weathadock -ErrorAction SilentlyContinue)) } else { Say 'WeathaDock is not installed in the usual place' }
$file = Join-Path $env:APPDATA 'app.weathadock.desktop\settings.json'
if (Test-Path $file) {
  try {
    $j = Get-Content $file -Raw | ConvertFrom-Json
    $places = @($j.locations)
    $kinds = @($places | ForEach-Object { 'zip ' + $(if ("$($_.zip)" -match '^\d{5}$') { 'ok' } else { 'odd' }) + ', position ' + $(if ($_.lat -is [ValueType] -and $_.lon -is [ValueType]) { 'ok' } else { 'odd' }) }) -join '; '
    Say ("settings: version=$($j.version) placement=$($j.placement) coveredMode=$($j.coveredMode) showWidget=$($j.showWidget) glass=$($j.glass) mouseBattery=$($j.mouseBattery) strip=" + ($j.taskbar | ConvertTo-Json -Compress) + " stripMovedTo=" + ($j.pillPos | ConvertTo-Json -Compress) + " places=$($places.Count) ($kinds)")
  } catch { Say 'settings: the file could not be read' }
} else { Say 'settings: no file' }
$crash = Join-Path $env:APPDATA 'app.weathadock.desktop\crash.log'
if (Test-Path $crash) { Say ('crash.log ends: ' + ((Get-Content $crash | Select-Object -Last 6) -join ' | ')) } else { Say 'crash.log: none' }
$stops = @(Get-WinEvent -FilterHashtable @{ LogName = 'Application'; ProviderName = 'Application Error', 'Application Hang'; StartTime = (Get-Date).AddDays(-2) } -ErrorAction SilentlyContinue | Where-Object { $_.Message -match 'weathadock|msedgewebview2' })
Say "Windows recorded the app or its web view stopping $($stops.Count) times in the last two days"
foreach ($e in ($stops | Select-Object -First 6)) { Say ('  ' + $e.TimeCreated.ToString('MM-dd HH:mm:ss') + ' ' + ((($e.Message -split "`n") | Select-Object -First 3 | ForEach-Object { $_.Trim() }) -join ' ; ')) }

$text = $out -join "`r`n"
if ($env:USERNAME) { $text = $text -replace [regex]::Escape($env:USERNAME), 'me' }
try { Set-Clipboard -Value $text; 'Copied. Paste it into the chat.' } catch { 'Could not copy it. Select the text below and copy it by hand.' }
''
$text
