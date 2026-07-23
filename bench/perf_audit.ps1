# bench/perf_audit.ps1 — automated load audit of a running ailimits.exe.
#
# Proves (or disproves) the performance budget claimed in the README:
# idle CPU ~0%, zero GPU engine usage, small working set, no raised platform
# timer resolution, negligible SRUM energy. Methodology notes:
#   * CPU is measured two independent ways: (1) exact kernel accounting —
#     TotalProcessorTime delta over the window (not sampling), and
#     (2) 1 Hz sampling of the formatted perf data, for the max spike.
#   * GPU: "GPU Engine(pid_*)" counter instances only exist for processes
#     that ever touched a GPU engine; their absence for our PID is itself
#     the proof of 0% GPU.
#   * Context switches across the process' threads approximate idle wakeups
#     (Bruce Dawson's methodology: efficiency == long core sleeps). Measured
#     as a raw-counter delta between two snapshots — enumerating every
#     thread's cooked data each second is far too slow (~45 s per query).
#   * powercfg /energy lists processes holding an elevated platform timer
#     resolution; powercfg /srumutil dumps per-app energy estimation.
#     Both need an elevated shell — skipped gracefully otherwise.
# CIM (Win32_PerfFormattedData_*) is used instead of Get-Counter paths so the
# script works on localized Windows too.
#
# Usage:  pwsh -File bench\perf_audit.ps1 [-DurationSec 300] [-OutDir bench\results]

param(
    [string]$ProcessName = "ailimits",
    [int]$DurationSec = 300,
    [int]$IntervalSec = 1,
    [string]$OutDir = "$PSScriptRoot\results"
)

$ErrorActionPreference = "Stop"
$proc = Get-Process -Name $ProcessName -ErrorAction Stop | Select-Object -First 1
$procId = $proc.Id
$cores = [Environment]::ProcessorCount
New-Item -ItemType Directory -Force $OutDir | Out-Null
$stamp = Get-Date -Format "yyyy-MM-dd_HHmm"

$elevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

Write-Host "Auditing PID $procId ($ProcessName) for $DurationSec s on $cores logical cores (elevated: $elevated)..."

# ── Exact CPU accounting: start snapshot ─────────────────────────────
$cpuStart = (Get-Process -Id $procId).TotalProcessorTime
$wallStart = Get-Date

# Raw thread snapshot for the context-switch delta (cumulative counter).
function Get-CswTotal {
    (Get-CimInstance Win32_PerfRawData_PerfProc_Thread `
        -Filter "Name LIKE '$ProcessName%'" -ErrorAction SilentlyContinue |
        Measure-Object ContextSwitchesPersec -Sum).Sum
}
$cswStart = Get-CswTotal

# ── 1 Hz sampling loop ───────────────────────────────────────────────
$samples = [System.Collections.Generic.List[object]]::new()
$gpuSeen = $false
$end = (Get-Date).AddSeconds($DurationSec)
while ((Get-Date) -lt $end) {
    $t0 = Get-Date
    $pd = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$procId"
    # GPU engine instances for this PID (utilization only exists if the
    # process ever submitted GPU work). Filter pushed down to WMI.
    $gpu = Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine `
        -Filter "Name LIKE 'pid_${procId}_%'" -ErrorAction SilentlyContinue
    $gpuUtil = 0
    if ($gpu) { $gpuSeen = $true; $gpuUtil = ($gpu | Measure-Object UtilizationPercentage -Sum).Sum }
    if ($pd) {
        $samples.Add([pscustomobject]@{
            t          = $t0.ToString("HH:mm:ss")
            cpu_pct    = [math]::Round($pd.PercentProcessorTime / $cores, 3) # % of the whole machine
            ws_mb      = [math]::Round($pd.WorkingSet / 1MB, 2)
            priv_mb    = [math]::Round($pd.PrivateBytes / 1MB, 2)
            threads    = $pd.ThreadCount
            handles    = $pd.HandleCount
            io_bps     = $pd.IODataBytesPersec
            gpu_pct    = $gpuUtil
        })
    }
    $sleep = $IntervalSec - ((Get-Date) - $t0).TotalSeconds
    if ($sleep -gt 0) { Start-Sleep -Seconds $sleep }
}

# ── Exact CPU accounting: end snapshot ───────────────────────────────
$cpuEnd = (Get-Process -Id $procId).TotalProcessorTime
$wallSec = ((Get-Date) - $wallStart).TotalSeconds
$cpuSec = ($cpuEnd - $cpuStart).TotalSeconds
$cswEnd = Get-CswTotal
$cswPerSec = if ($null -ne $cswStart -and $null -ne $cswEnd) {
    [math]::Round(($cswEnd - $cswStart) / $wallSec, 2)
} else { $null }
$cpuOfOneCore = 100 * $cpuSec / $wallSec
$cpuOfMachine = $cpuOfOneCore / $cores

# ── powercfg checks (need admin) ─────────────────────────────────────
$timerVerdict = "skipped (needs elevated shell)"
$energyRows = @()
if ($elevated) {
    $energyHtml = Join-Path $OutDir "energy-$stamp.html"
    & powercfg /energy /output $energyHtml /duration 60 | Out-Null
    $html = Get-Content $energyHtml -Raw
    $timerVerdict = if ($html -match "(?i)outstanding (kernel )?timer request[\s\S]{0,600}?$ProcessName") {
        "FAIL: $ProcessName holds an elevated platform timer resolution"
    } else {
        "PASS: $ProcessName not present among timer-resolution requesters"
    }
    $srum = Join-Path $OutDir "srum-$stamp.csv"
    & powercfg /srumutil /output $srum /csv | Out-Null
    $energyRows = Import-Csv $srum | Where-Object { $_.AppId -like "*$ProcessName*" } |
        Select-Object -Last 24
}

# ── Aggregate + report ───────────────────────────────────────────────
function Stat($prop) {
    $m = $samples | Measure-Object $prop -Average -Maximum -Minimum
    [pscustomobject]@{ avg = [math]::Round($m.Average, 3); max = $m.Maximum; min = $m.Minimum }
}
$cpu = Stat "cpu_pct"; $ws = Stat "ws_mb"; $thr = Stat "threads"
$hnd = Stat "handles"; $io = Stat "io_bps"; $gpu = Stat "gpu_pct"

$os = Get-CimInstance Win32_OperatingSystem
$cpuName = (Get-CimInstance Win32_Processor).Name
$report = [pscustomobject]@{
    date               = (Get-Date).ToString("yyyy-MM-dd HH:mm")
    machine            = @{ os = $os.Caption; build = $os.BuildNumber; cpu = $cpuName; logical_cores = $cores }
    process            = @{ name = $ProcessName; pid = $procId; duration_sec = [math]::Round($wallSec, 1); samples = $samples.Count }
    cpu_exact          = @{
        cpu_seconds_total  = [math]::Round($cpuSec, 3)
        pct_of_one_core    = [math]::Round($cpuOfOneCore, 4)
        pct_of_machine     = [math]::Round($cpuOfMachine, 4)
    }
    cpu_sampled_pct    = $cpu
    gpu_engine         = @{ instances_for_pid = $gpuSeen; utilization_pct = $gpu }
    working_set_mb     = $ws
    threads            = $thr
    handles            = $hnd
    io_bytes_per_sec   = $io
    context_switches_per_sec = $cswPerSec
    platform_timer     = $timerVerdict
    srum_energy_rows   = $energyRows
}
$json = Join-Path $OutDir "perf-audit-$stamp.json"
$report | ConvertTo-Json -Depth 6 | Set-Content $json
$samples | Export-Csv (Join-Path $OutDir "perf-audit-$stamp.samples.csv") -NoTypeInformation

Write-Host ""
Write-Host "== ailimits load audit ($([math]::Round($wallSec,0)) s, $($samples.Count) samples) =="
Write-Host ("CPU exact : {0:N3} CPU-seconds total = {1:N4}% of one core = {2:N4}% of machine" -f $cpuSec, $cpuOfOneCore, $cpuOfMachine)
Write-Host ("CPU sample: avg {0}% / max {1}% of machine" -f $cpu.avg, $cpu.max)
Write-Host ("GPU       : engine instances for PID: {0}; utilization avg {1}% max {2}%" -f $gpuSeen, $gpu.avg, $gpu.max)
Write-Host ("Memory    : working set avg {0} MB (max {1} MB)" -f $ws.avg, $ws.max)
Write-Host ("Threads   : avg {0} (max {1});  Handles: avg {2} (max {3})" -f $thr.avg, $thr.max, $hnd.avg, $hnd.max)
Write-Host ("I/O       : avg {0} B/s (max {1} B/s)" -f [math]::Round($io.avg,0), $io.max)
Write-Host ("CtxSw/sec : {0}  [idle-wakeup proxy, whole-window average]" -f $cswPerSec)
Write-Host ("Timer res : {0}" -f $timerVerdict)
Write-Host "JSON: $json"
