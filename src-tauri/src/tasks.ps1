# Lists third-party scheduled tasks that start at logon (as JSON).
$ErrorActionPreference = 'SilentlyContinue'
$me = [Security.Principal.WindowsIdentity]::GetCurrent().Name.Split('\')[-1]
$out = Get-ScheduledTask | Where-Object {
    $_.TaskPath -notlike '\Microsoft\*' -and $_.TaskPath -notlike '\h3nrx\*' -and
    ($_.Triggers | Where-Object { $_.CimClass.CimClassName -eq 'MSFT_TaskLogonTrigger' })
} | ForEach-Object {
    $a = $_.Actions | Select-Object -First 1
    $user = "$($_.Principal.UserId)"
    [pscustomobject]@{
        path    = $_.TaskPath + $_.TaskName
        name    = $_.TaskName
        exe     = "$($a.Execute)".Trim('"').Replace('\\', '\')
        args    = "$($a.Arguments)"
        enabled = $_.State -ne 'Disabled'
        system  = -not ($user -like "*$me*") -or "$($_.Principal.RunLevel)" -eq 'Highest'
    }
}
@($out) | ConvertTo-Json -Compress
