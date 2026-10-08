# M0 spike 2 (Windows): does system pointer acceleration change injected relative moves?
# Throwaway. Requires a running agent (real input) and an lpctl already paired to it.
# Moves the real cursor: do not touch the mouse while it runs.
param(
    [string]$Lpctl = "target\release\lpctl.exe",
    [string]$CtlHome = "$env:TEMP\lp-spike-ctl",
    [string]$Server = "spike",
    [int]$Distance = 400,
    [int[]]$Steps = @(100, 40, 10, 4),
    [int]$Repeats = 3
)

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$mouse = Get-ItemProperty "HKCU:\Control Panel\Mouse"
$accelOn = $mouse.MouseSpeed -ne "0"
"Enhance pointer precision: $accelOn (MouseSpeed=$($mouse.MouseSpeed) Threshold1=$($mouse.MouseThreshold1) Threshold2=$($mouse.MouseThreshold2)); pointer speed slider: $($mouse.MouseSensitivity)/20"

$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$startX = [int]($screen.Width / 2 - $Distance / 2)
$startY = [int]($screen.Height / 2)

$rows = @()
foreach ($n in $Steps) {
    for ($r = 1; $r -le $Repeats; $r++) {
        [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point($startX, $startY)
        Start-Sleep -Milliseconds 250
        & $Lpctl --home $CtlHome move $Server $Distance 0 --steps $n | Out-Null
        Start-Sleep -Milliseconds 300
        $end = [System.Windows.Forms.Cursor]::Position
        $rows += [pscustomobject]@{
            steps = $n
            px_per_frame = [math]::Round($Distance / $n, 1)
            repeat = $r
            sent = $Distance
            moved = $end.X - $startX
            ratio = [math]::Round(($end.X - $startX) / $Distance, 3)
        }
    }
}
$rows | Format-Table -AutoSize
$out = "spikes\m0\accel\windows-accel-$(if ($accelOn) { 'on' } else { 'off' }).csv"
$rows | Export-Csv -NoTypeInformation -Path $out
"saved $out"
