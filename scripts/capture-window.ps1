# Captura la ventana de Silb con PrintWindow, que pinta la ventana en un bitmap
# aunque otra app la tape: CopyFromScreen copia lo que haya encima (la primera
# version de este script guardo un pantallazo de Steam).
# Requiere la app arrancada (si no, la lanza y espera).
# Uso: powershell -ExecutionPolicy Bypass -File scripts/capture-window.ps1 [-Salida docs/img/ventana-principal.png] [-exe target/release/silb.exe]

param(
  [string]$Salida = "docs\img\ventana-principal.png",
  [string]$exe = "target\release\silb.exe"
)

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class Win {
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  public struct RECT { public int Left, Top, Right, Bottom; }

  public static List<IntPtr> Ventanas(uint[] pids) {
    var lista = new List<IntPtr>();
    EnumWindows((h, p) => {
      uint w; GetWindowThreadProcessId(h, out w);
      foreach (var pid in pids) if (pid == w) { lista.Add(h); break; }
      return true;
    }, IntPtr.Zero);
    return lista;
  }
}
"@

# 1) ¿Hay ya un silb corriendo? Si no, se lanza y se espera a que aparezca.
$procs = @(Get-Process -Name silb -ErrorAction SilentlyContinue)
if ($procs.Count -eq 0) {
  Start-Process -FilePath "$exe" | Out-Null
  for ($i = 0; $i -lt 20; $i++) {
    Start-Sleep -Milliseconds 700
    $procs = @(Get-Process -Name silb -ErrorAction SilentlyContinue)
    if ($procs.Count -gt 0) { break }
  }
}
Start-Sleep -Seconds 9   # deja cargar el modelo
$procs = @(Get-Process -Name silb -ErrorAction SilentlyContinue)
$pids = @($procs | ForEach-Object { [uint32]$_.Id })
Write-Output ("procesos silb: " + ($pids -join ","))

$mejor = [IntPtr]::Zero; $area = 0; $titulo = ""
foreach ($h in [Win]::Ventanas($pids)) {
  $sb = New-Object System.Text.StringBuilder 256
  [void][Win]::GetWindowTextW($h, $sb, 256)
  $t = $sb.ToString()
  $r = New-Object Win+RECT
  [void][Win]::GetWindowRect($h, [ref]$r)
  $a = ($r.Right - $r.Left) * ($r.Bottom - $r.Top)
  $vis = [Win]::IsWindowVisible($h)
  Write-Output ("  ventana '{0}' {1}x{2} visible={3}" -f $t, ($r.Right-$r.Left), ($r.Bottom-$r.Top), $vis)
  if ($vis -and $t -ne "" -and $a -gt $area) { $area = $a; $mejor = $h; $titulo = $t }
}
if ($mejor -eq [IntPtr]::Zero) { Write-Output "sin ventana"; exit 1 }

[void][Win]::ShowWindow($mejor, 9)
$r2 = New-Object Win+RECT
[void][Win]::GetWindowRect($mejor, [ref]$r2)
$w = $r2.Right - $r2.Left; $hh = $r2.Bottom - $r2.Top

# 2) PrintWindow
$bmp = New-Object System.Drawing.Bitmap($w, $hh)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
$ok = [Win]::PrintWindow($mejor, $hdc, 2)
$g.ReleaseHdc($hdc); $g.Dispose()
$bmp.Save($Salida, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
Write-Output "printwindow=$ok ${w}x${hh} '$titulo' -> $Salida"
