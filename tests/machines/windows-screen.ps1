<#
.SYNOPSIS
  Says what a Windows machine's screen shows, to compare with the picture away-try takes.

.DESCRIPTION
  Twice a second it takes the screen as the machine shows it and writes one line: the
  time, the screen's size, and the SHA-256 of its pixels, three bytes each (blue, green,
  red), row by row from the top. away-try says the same sum of the picture it took.
  Where the two are equal, the picture is the machine's screen, pixel for pixel.

  A screen belongs to a session, so this has to run in the session of the account that
  away-try signs in as. Run it there, or name the account as an administrator of the
  machine: it then starts itself in that account's session with a scheduled task, waits,
  writes the lines, and removes the task and its files again. The account has to have
  signed in before, so that it has a session.

  A session nobody is connected to has no screen: the lines begin when away-try connects.

.PARAMETER ForAccount
  The account whose screen is taken, as DOMAIN\name. Without it: the screen of the
  session this runs in.

.PARAMETER Seconds
  For how long screens are taken.

.PARAMETER Pictures
  A folder to keep every screen in as well, as PNG files, to look at where a picture
  differs. It is made if it isn't there.

.EXAMPLE
  .\windows-screen.ps1 -ForAccount 'CORP\alex.urban' -Seconds 40

.EXAMPLE
  Invoke-Command -ComputerName fin-app-02.example.com -FilePath .\windows-screen.ps1 -ArgumentList 'CORP\alex.urban', 40
#>
param(
  [string]$ForAccount,
  [int]$Seconds = 30,
  [string]$Pictures,
  # where the lines go when this runs as the task it started itself
  [string]$Lines
)

$ErrorActionPreference = 'Stop'
$head = 'time,width,height,sha256'

if ($ForAccount) {
  $folder = Join-Path $env:ProgramData 'away-screen'
  $script = Join-Path $folder 'windows-screen.ps1'
  $lines = Join-Path $folder 'lines.csv'
  $task = 'away-screen'
  New-Item -ItemType Directory -Force $folder | Out-Null
  Set-Content -Path $script -Value $MyInvocation.MyCommand.ScriptBlock.ToString() -Encoding UTF8
  Set-Content -Path $lines -Value $head
  icacls $folder /grant "${ForAccount}:(OI)(CI)M" | Out-Null
  $words = "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$script`" -Seconds $Seconds -Lines `"$lines`""
  if ($Pictures) {
    New-Item -ItemType Directory -Force $Pictures | Out-Null
    icacls $Pictures /grant "${ForAccount}:(OI)(CI)M" | Out-Null
    $words += " -Pictures `"$Pictures`""
  }
  $runs = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $words
  $as = New-ScheduledTaskPrincipal -UserId $ForAccount -LogonType Interactive -RunLevel Limited
  Register-ScheduledTask -TaskName $task -Action $runs -Principal $as -Force | Out-Null
  try {
    Start-ScheduledTask -TaskName $task
    Start-Sleep -Seconds ($Seconds + 3)
    Get-Content $lines
  } finally {
    Stop-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $task -Confirm:$false
    Remove-Item $folder -Recurse -Force
  }
  return
}

Add-Type -AssemblyName System.Drawing, System.Windows.Forms
# without this a session that enlarges its screen would give a picture of another size
Add-Type -Namespace AwayScreen -Name Native -MemberDefinition '[DllImport("user32.dll")] public static extern bool SetProcessDPIAware();'
[AwayScreen.Native]::SetProcessDPIAware() | Out-Null
if ($Pictures) { New-Item -ItemType Directory -Force $Pictures | Out-Null }
if (-not $Lines) { $head }

$until = (Get-Date).AddSeconds($Seconds)
while ((Get-Date) -lt $until) {
  $at = Get-Date
  try {
    $screen = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $format = [System.Drawing.Imaging.PixelFormat]::Format24bppRgb
    $bitmap = New-Object System.Drawing.Bitmap $screen.Width, $screen.Height, $format
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($screen.Left, $screen.Top, 0, 0, $bitmap.Size)
    $graphics.Dispose()

    # the pixels as they lie in memory: blue, green, red, each row filled up to whole words
    $all = New-Object System.Drawing.Rectangle 0, 0, $bitmap.Width, $bitmap.Height
    $held = $bitmap.LockBits($all, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, $format)
    $bytes = New-Object byte[] ($held.Stride * $held.Height)
    [System.Runtime.InteropServices.Marshal]::Copy($held.Scan0, $bytes, 0, $bytes.Length)
    $bitmap.UnlockBits($held)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    for ($row = 0; $row -lt $held.Height; $row++) {
      [void]$sha.TransformBlock($bytes, $row * $held.Stride, $bitmap.Width * 3, $null, 0)
    }
    [void]$sha.TransformFinalBlock($bytes, 0, 0)
    $sum = -join ($sha.Hash | ForEach-Object { $_.ToString('x2') })

    $line = '{0:HH:mm:ss.fff},{1},{2},{3}' -f $at, $bitmap.Width, $bitmap.Height, $sum
    if ($Lines) { Add-Content -Path $Lines -Value $line } else { $line }
    if ($Pictures) {
      $bitmap.Save((Join-Path $Pictures ('{0:HHmmss.fff}.png' -f $at)), [System.Drawing.Imaging.ImageFormat]::Png)
    }
    $bitmap.Dispose()
  } catch {
    # a session nobody is connected to has no screen to take
  }
  Start-Sleep -Milliseconds 400
}
