# Adds or removes CowMark's install folder on the current user's PATH, so `cowmark` works in new
# terminals. Run by the installer (cowmark-path.nsh).
#
# PATH is read unexpanded and written back with the same value kind, so entries such as
# %USERPROFILE%\... keep working, and there's no length limit (NSIS strings stop at 1024
# characters). Only an exact match of the folder is removed.
param(
  [Parameter(Mandatory = $true)] [ValidateSet('add', 'remove')] [string] $Action,
  [Parameter(Mandatory = $true)] [string] $Dir,
  # Tests point this at a scratch key.
  [string] $Key = 'Environment'
)
$ErrorActionPreference = 'Stop'

$folder = $Dir.TrimEnd('\')
$registry = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($Key)
try {
  $name = $registry.GetValueNames() | Where-Object { $_ -ieq 'Path' } | Select-Object -First 1
  if ($name) {
    $old = [string]$registry.GetValue($name, '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $kind = $registry.GetValueKind($name)
  } else {
    $name = 'Path'
    $old = ''
    $kind = [Microsoft.Win32.RegistryValueKind]::ExpandString
  }
  $parts = @($old -split ';' | Where-Object { $_ -ne '' })
  $others = @($parts | Where-Object { $_.TrimEnd('\') -ine $folder })

  if ($Action -eq 'add') {
    if ($others.Count -lt $parts.Count) { exit 0 }
    $registry.SetValue($name, (($parts + $folder) -join ';'), $kind)
  } else {
    if ($others.Count -eq $parts.Count) { exit 0 }
    if ($others.Count -eq 0) {
      $registry.DeleteValue($name)
    } else {
      $registry.SetValue($name, ($others -join ';'), $kind)
    }
  }
} finally {
  $registry.Close()
}
