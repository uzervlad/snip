# snip

"simple" gui for trimming video written in ~~very poor~~ rust

requires ffmpeg

to use run `snip <video_path>`

### why

yes

### issues

* ~~i suck at rust~~
* ffmpeg console keeps appearing with `windows_subsystem = "windows"`
* layout is fucking awful
* audio/video desync - n00kii/egui-video#7

### keybinds

* space - play/pause
* s - set start
* e - set end
* a - cycle audio channel
* left/right arrows - seek 5s (shift = 1s)
* enter - ***snip***
