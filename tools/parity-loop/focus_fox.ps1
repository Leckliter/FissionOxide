# Usage: pwsh tools/parity-loop/focus_fox.ps1 [-x X -y Y]
param([int]$x = -417, [int]$y = 726)  # an empty spot on FissionOxide's canvas, virtual-screen coords
# Gives FissionOxide focus back from TextInputHost by clicking an empty spot of its own canvas,
# only if FissionOxide's window is the one under that point.
Add-Type -Namespace FF -Name U -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
[DllImport("user32.dll")] public static extern void mouse_event(uint f,int x,int y,uint d,UIntPtr e);
[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(long p);
[DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint f);
'@
$fox = (Get-Process fissionoxide | Where-Object MainWindowHandle -ne 0 | Select-Object -First 1).MainWindowHandle
$packed = ([long]$y -shl 32) -bor ([long]$x -band 0xFFFFFFFF)
$w = [FF.U]::GetAncestor([FF.U]::WindowFromPoint($packed), 2)
if ($w -ne $fox) { "not fox under point"; exit 1 }
[FF.U]::SetCursorPos($x, $y) | Out-Null
[FF.U]::mouse_event(2,0,0,0,[UIntPtr]::Zero); [FF.U]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 300
"fox focused: $([FF.U]::GetForegroundWindow() -eq $fox)"
