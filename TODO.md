# TODO

## Pre-1.0
- [x] BUG -> skipping song should set state.pause=false
- [x] History (Key: <), Just play back
- [x] Quick actions popup for moving up/removing up next
- [x] All music right side
- [x] Add/delete to/from queue
- [x] Large refactor
- [x] README
- [x] Add concurrency to initial ffprobe to increase speeds
- [x] Move drawing to a different thread to allow for frame rate regulation
- [x] Add configuration for color theming (need XDG standard crate)
- [x] Selection screen for custom music folder -> moved to configuration
- [x] Allow regeneration/rescanning of Music directory
- [x] Error/alert pop up
- [x] Add clap to allow for `-d|--dir` flag to pick music directory
- [x] Fzf searching in a given pane (search box top right, 3 line constraint)

## Post-1.0
- [x] Add seek/skip functionality with left/right keys
- [x] Only allow audio file types when scanning music directory (not necessary)
- [x] Add album metadata and sorting
- [x] Add MPRIS functionality
- [ ] Add album view with hot key for pane switching with library
- [ ] Add in playlist support for `-p|--playlist`
- [ ] Clean up filtering/music data distinction on Songs access
- [ ] Refactor widgets to handle constraints in their build function
- [ ] Refactor main error handling loop to accumulate errors 
- [ ] Arch PKGBUILD automation
