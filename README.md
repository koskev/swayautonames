# Swayautonames
This little program automatically renames workspaces in sway similar to [i3-workspace-names-daemon](https://github.com/i3-workspace-names-daemon/i3-workspace-names-daemon).

Example:

![pic](images/pic.png)

# Configuration
Configuration files are loaded with the priority

 - `./config.json`
 - `$XDG_CONFIG_HOME/swayautonames/config.json`
 - `/etc/swayautonames/config.json`

The config has the form
```
{
    "app_symbols": {
        "app_name": "symbol"
    }
}
```

For the sway configuration you should be using numbered Workspaces instead of names.
E.g.
```
bindsym $mod+1 workspace number 1
```

# Supported window managers
 - Sway
 - Hyprland
