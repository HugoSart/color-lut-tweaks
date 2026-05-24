# Windows Auto Color Management (ACM) Exploration

## Goal

Investigate whether `color-lut-tweaks` can enable or disable Windows Auto Color Management (ACM) from code and apply the change without requiring a full Windows restart.

## Registry Finding

Windows stores a monitor-specific ACM value under:

```text
HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers\MonitorDataStore\<monitor-key>\AutoColorManagementEnabled
```

Observed values:

```text
0 = ACM disabled
1 = ACM enabled
```

Writing this value requires administrator privileges because it is under `HKLM`.

## Experimental Script

Script:

```text
.agents/workspace/scripts/set-acm.ps1
```

Useful commands:

```powershell
powershell -ExecutionPolicy Bypass -File .agents\workspace\scripts\set-acm.ps1 -Mode Status
powershell -ExecutionPolicy Bypass -File .agents\workspace\scripts\set-acm.ps1 -Mode On
powershell -ExecutionPolicy Bypass -File .agents\workspace\scripts\set-acm.ps1 -Mode Off
powershell -ExecutionPolicy Bypass -File .agents\workspace\scripts\set-acm.ps1 -Mode On -RestartMonitorDevice
powershell -ExecutionPolicy Bypass -File .agents\workspace\scripts\set-acm.ps1 -Mode Off -RestartMonitorDevice
```

## Refresh Attempts

The registry write alone changed persisted state, but Windows did not immediately apply the UI/runtime behavior.

The following refresh attempts did not apply the ACM change immediately:

```text
WM_SETTINGCHANGE broadcast
ChangeDisplaySettingsEx with the current display mode
Win + Ctrl + Shift + B graphics driver reset
```

The ACM change did apply after a full Windows restart.

## Working Apply-Now Path

After writing the registry value, physically turning the monitor off and on caused Windows to apply the ACM state without a full restart.

The script was then updated to emulate this by restarting the monitor PnP device:

```powershell
Disable-PnpDevice -InstanceId <monitor-instance-id> -Confirm:$false
Start-Sleep -Seconds 2
Enable-PnpDevice -InstanceId <monitor-instance-id> -Confirm:$false
```

This worked.

Conclusion: Windows appears to reload the ACM state when the monitor/display target is re-enumerated. The registry value is persistent configuration, but the active display pipeline does not reload it from a normal settings broadcast or display mode refresh.

## Viability

ACM control is viable as an experimental/admin feature:

```text
write ACM registry value
restart selected monitor PnP device
ACM applies without logout/restart
```

It should not be a silent/default tray behavior because it:

```text
requires administrator privileges
briefly disconnects/reconnects the monitor
may disturb display layout or windows
may fail depending on monitor/driver behavior
```

## Suggested App Design

Config shape could eventually look like:

```json
{
  "windows": {
    "autoColorManagement": "off"
  }
}
```

or:

```json
{
  "windows": {
    "autoColorManagement": {
      "desired": "off",
      "applyNow": "restart-monitor-device"
    }
  }
}
```

Recommended UX:

```text
Detect desired ACM state mismatch
Show tray warning/status
Offer explicit "Apply ACM now" action
Prompt for elevation
Restart only selected monitor device
Tell user the display may blink/reconnect
```

## Open Questions

- How reliably does PnP monitor restart work across monitors, GPUs, and Windows builds?
- Can the monitor PnP device be mapped cleanly to the app's display device index/name?
- Does the behavior differ for internal laptop panels vs external monitors?
- Does the setting behave differently when HDR is enabled?
- Is there a private Windows API used by Settings that applies ACM without PnP restart?
