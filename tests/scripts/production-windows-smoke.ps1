param([Parameter(Mandatory = $true)][string]$Executable)
$ErrorActionPreference = 'Stop'
function Mark-Stage([string]$stage) {
    [Console]::Out.WriteLine("Production UI stage: $stage")
    [Console]::Out.Flush()
}
Mark-Stage 'load UIA assemblies'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
# UIA invokes callbacks on its own threads; C# signals a handle without a PowerShell runspace.
Mark-Stage 'compile UIA observers'
Add-Type -ReferencedAssemblies @(
    [System.Windows.Automation.AutomationElement].Assembly.Location,
    [System.Windows.Automation.AutomationProperty].Assembly.Location
) -TypeDefinition @'
using System;
using System.Diagnostics;
using System.Threading;
using System.Windows.Automation;

public sealed class ProductionUiChanges : IDisposable {
    private readonly AutoResetEvent changed = new AutoResetEvent(false);
    private readonly AutomationElement root = AutomationElement.RootElement;
    private readonly AutomationEventHandler opened;
    private readonly StructureChangedEventHandler structure;
    private readonly AutomationPropertyChangedEventHandler property;
    private readonly EventHandler exited;
    private Process process;
    public ProductionUiChanges() {
        opened = (s, e) => Signal();
        structure = (s, e) => Signal();
        property = (s, e) => Signal();
        exited = (s, e) => Signal();
        Automation.AddAutomationEventHandler(WindowPattern.WindowOpenedEvent, root, TreeScope.Subtree, opened);
        Automation.AddStructureChangedEventHandler(root, TreeScope.Subtree, structure);
        Automation.AddAutomationPropertyChangedEventHandler(root, TreeScope.Subtree, property,
            AutomationElement.NameProperty, AutomationElement.IsEnabledProperty, AutomationElement.IsOffscreenProperty);
    }
    private void Signal() {
        try { changed.Set(); } catch (ObjectDisposedException) { }
    }
    public void Watch(Process target) {
        process = target;
        process.Exited += exited;
        process.EnableRaisingEvents = true;
    }
    public void Wait(int milliseconds) { changed.WaitOne(milliseconds); }
    public void Dispose() {
        Automation.RemoveAutomationEventHandler(WindowPattern.WindowOpenedEvent, root, opened);
        Automation.RemoveStructureChangedEventHandler(root, structure);
        Automation.RemoveAutomationPropertyChangedEventHandler(root, property);
        if (process != null) process.Exited -= exited;
        changed.Dispose();
    }
}
'@

Mark-Stage 'load locale catalogues'
$labels = Get-ChildItem clients/desktop/ui/i18n/*.json | ForEach-Object {
    $catalogue = Get-Content -Raw -Encoding UTF8 $_.FullName | ConvertFrom-Json
    @{ heading = $catalogue.'setup.title'; button = $catalogue.'setup.create' }
}
if (-not $labels) { throw 'Missing production smoke locale catalogues' }

function Find-Setup($window, $expectedLabels) {
    foreach ($locale in $expectedLabels) {
        $headingCondition = [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::NameProperty, [string]$locale.heading)
        $buttonCondition = [System.Windows.Automation.AndCondition]::new(
            [System.Windows.Automation.PropertyCondition]::new(
                [System.Windows.Automation.AutomationElement]::NameProperty, [string]$locale.button),
            [System.Windows.Automation.PropertyCondition]::new(
                [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
                [System.Windows.Automation.ControlType]::Button))
        $heading = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $headingCondition)
        $button = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $buttonCondition)
        if ($heading -and $button -and -not $heading.Current.IsOffscreen -and
            -not $button.Current.IsOffscreen -and $button.Current.IsEnabled) { return $true }
    }
    return $false
}

function Get-AppWindow {
    if ($appProcess.MainWindowHandle -eq [IntPtr]::Zero) { return $null }
    $candidate = [System.Windows.Automation.AutomationElement]::FromHandle($appProcess.MainWindowHandle)
    if ($candidate.Current.ProcessId -eq $appProcess.Id) { return $candidate }
}

function Await-Ready([string]$stage, [scriptblock]$probe) {
    Mark-Stage $stage
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    while ($true) {
        $appProcess.Refresh()
        if ($appProcess.HasExited) { throw "Production app exited during $stage; code=$($appProcess.ExitCode)" }
        try {
            $value = & $probe
            if ($value) { return $value }
        } catch [System.Windows.Automation.ElementNotAvailableException] {
            # A provider may be replaced during startup; inspect again on the next UIA signal.
        }
        $remaining = 45000 - $clock.ElapsedMilliseconds
        if ($remaining -le 0) { throw "Production UI transition not ready: $stage" }
        $changes.Wait([int]$remaining)
    }
}

function Show-ObservedTree {
    $appProcess.Refresh()
    Write-Output "Production process id=$($appProcess.Id) exited=$($appProcess.HasExited) handle=$($appProcess.MainWindowHandle)"
    Write-Output "Requested profile APPDATA=$env:APPDATA LOCALAPPDATA=$env:LOCALAPPDATA"
    Write-Output "Known config base=$([Environment]::GetFolderPath([Environment+SpecialFolder]::ApplicationData))"
    if ($appProcess.MainWindowHandle -eq [IntPtr]::Zero) { return }
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($appProcess.MainWindowHandle)
    $pending = [System.Collections.Generic.Queue[System.Windows.Automation.AutomationElement]]::new()
    $pending.Enqueue($root)
    $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
    $count = 0
    while ($pending.Count -gt 0 -and $count -lt 256) {
        $node = $pending.Dequeue()
        $count++
        $current = $node.Current
        Write-Output ($current | Select-Object ProcessId, Name, AutomationId, ControlType, IsOffscreen, IsEnabled |
            ConvertTo-Json -Compress -Depth 2)
        $child = $walker.GetFirstChild($node)
        while ($child -and $pending.Count -lt 256) {
            $pending.Enqueue($child)
            $child = $walker.GetNextSibling($child)
        }
    }
    if ($pending.Count -gt 0) { Write-Output 'Production UI tree truncated at 256 nodes' }
}

$profileDir = Join-Path $env:RUNNER_TEMP ('production-desktop-' + [guid]::NewGuid())
$env:APPDATA = Join-Path $profileDir 'roaming'
$env:LOCALAPPDATA = Join-Path $profileDir 'local'
New-Item -ItemType Directory -Force $env:APPDATA, $env:LOCALAPPDATA | Out-Null
Mark-Stage 'register UIA observers'
$changes = [ProductionUiChanges]::new()
$appProcess = $null
try {
    Mark-Stage 'launch production executable'
    $appProcess = Start-Process -FilePath (Resolve-Path $Executable) -PassThru
    $changes.Watch($appProcess)
    $null = Await-Ready 'native window' { Get-AppWindow }
    # A successful window transition starts a fresh render deadline; unrelated events do not extend it.
    $window = Await-Ready 'visible setup heading and enabled create button' {
        $currentWindow = Get-AppWindow
        if ($currentWindow -and (Find-Setup $currentWindow $labels)) { return $currentWindow }
    }
    $missingLabels = @(@{ heading = 'Missing smoke heading'; button = 'Missing smoke button' })
    if (Find-Setup $window $missingLabels) { throw 'Accessibility check accepted absent controls' }
    Mark-Stage 'close production window'
    if (-not $appProcess.CloseMainWindow()) { throw 'Could not close production window' }
    if (-not $appProcess.WaitForExit(15000)) { throw 'Production app did not exit after Close' }
    if ($appProcess.ExitCode -ne 0) { throw 'Production app exited with an error' }
    Write-Output 'Production desktop rendered setup and closed without injected hooks'
} catch {
    if ($appProcess) {
        try { Show-ObservedTree } catch { Write-Warning "Production UI diagnostics failed: $_" }
    }
    throw
} finally {
    $changes.Dispose()
    if ($appProcess -and -not $appProcess.HasExited) { $appProcess.Kill(); $appProcess.WaitForExit() }
}
