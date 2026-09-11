# Interactive test target ONLY. No global hooks, background persistence or file deletion.
[CmdletBinding()] param()
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'This fixture requires Windows desktop.' }
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[Windows.Forms.Application]::EnableVisualStyles()
$form = [Windows.Forms.Form]::new(); $form.Text='RemoteCodex input fixture - SAFE TEST TARGET'
$form.Size=[Drawing.Size]::new(760,520); $form.StartPosition='CenterScreen'; $form.KeyPreview=$true
$label=[Windows.Forms.Label]::new(); $label.Location=[Drawing.Point]::new(20,20); $label.Size=[Drawing.Size]::new(700,50)
$label.Text='Approve this window from the company UI. Click/type only in this fixture. No data is saved.'
$input=[Windows.Forms.TextBox]::new(); $input.Location=[Drawing.Point]::new(20,90); $input.Size=[Drawing.Size]::new(700,130); $input.Multiline=$true; $input.ScrollBars='Vertical'
$button=[Windows.Forms.Button]::new(); $button.Location=[Drawing.Point]::new(20,245); $button.Size=[Drawing.Size]::new(230,50); $button.Text='Click counter: 0'
$state=[Windows.Forms.Label]::new(); $state.Location=[Drawing.Point]::new(20,315); $state.Size=[Drawing.Size]::new(700,90); $state.Text='Keys / wheel / geometry appear here only; nothing is written to disk.'
$form.Tag=0
$button.Add_Click({ $form.Tag=[int]$form.Tag+1; $button.Text="Click counter: $($form.Tag)" })
$form.Add_KeyDown({ param($sender,$event) $state.Text="Key down: $($event.KeyCode) Modifiers: $($event.Modifiers)" })
$form.Add_KeyUp({ param($sender,$event) $state.Text="Key up: $($event.KeyCode) Modifiers: $($event.Modifiers)" })
$form.Add_MouseWheel({ param($sender,$event) $state.Text="Wheel delta: $($event.Delta)" })
$form.Add_Resize({ $state.Text="Window changed: $($form.Width) x $($form.Height). Remote input must stop until re-approved." })
$form.Controls.AddRange([Windows.Forms.Control[]]@($label,$input,$button,$state))
try { [void]$form.ShowDialog() } finally { $form.Dispose() }
