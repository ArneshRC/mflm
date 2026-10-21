# Minimal Framebuffer Login Manager

"A stupidly simple graphical login manager" based on [deathowl/ddlm](https://github.com/deathowl/ddlm).

Uses framebuffer, so You wont have to run a wayland session to bootstrap your wayland session (unlike gtkgreet)

This is a greetd frontend.

# Prior work:
Continuation/fork of prior work by [deathowl](https://github.com/deathowl/ddlm). This one uses a custom config file, no embedded fonts, and a new, simpler layout.

# Screenshot

![](assets/mflm_fbgrab.png)

# Setup

My greetd config looks like this:

```toml
[terminal]
# The VT to run the greeter on. Can be "next", "current" or a number
# designating the VT.
vt = 1

# The default session, also known as the greeter.
[default_session]

command = "mflm" 

# The user to run the command as. The privileges this user must have depends
# on the greeter. A graphical greeter may for example require the user to be
# in the `video` group.
user = "greeter"
```

And my mflm config looks like this:

```toml
[theme]
font = { family = "JetBrainsMono Nerd Font Mono" }

[theme.colors]
text = "#fafafa"
background = "#000000"
muted = "#cccccc"
accent = "#aaccee"
error = "#ecbcad"

[header]
font = { family = "Muli", size = 25 }

[form]
text_align = "left"

[form.session]
default = "Niri"

[form.username]
default = "myuser"
icon = "\uf007"

[form.password]
icon = "\uf084"
mask = "•"

[power]
font = { size = 25 }

[[power.actions]]
kind = "shutdown"
key = "F1"
icon = "󰤆"
label = "Shutdown"

[[power.actions]]
kind = "reboot"
key = "F2"
icon = ""
label = "Restart"
command = ["loginctl", "reboot"]

[[power.actions]]
kind = "suspend"
key = "F3"
icon = "󰤄"
label = "Sleep"

```

The icon font has to be installed system-wide (e.g. under `/usr/share/fonts`) since the greeter user can't see fonts in your home directory. A missing glyph is logged and the icon is left out. "Mono" Nerd Font variants draw icons at single-cell width; use the "Propo" variant or "Symbols Nerd Font" as `form.icons.font` for full-size icons.

# Future plans:
* Add modules on the headline (datetime etc.)

# AI Disclosure
Claude Opus 5.5 was used to assist with planning and implementation.
