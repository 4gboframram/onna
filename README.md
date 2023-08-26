# Onna - A real-time video player / streamer for the terminal

## Features

- Support for as many video formats as your `gstreamer` installation supports
- Supports any terminal that supports ANSI escape sequences for cursor movement and color

  - Truecolor and ANSI 256 color
- Real-time video playing / streaming with audio
- Frame dropping when the renderer gets out of sync

## Previews

✅ Bad Apple (click to view)

[![Bad Apple](https://img.youtube.com/vi/vuBwfK6ZA50/0.jpg)](https://www.youtube.com/watch?v=vuBwfK6ZA50)

✅ `女の子になりたい` (I wanna be a girl) (click to view)

[![Bad Apple](https://img.youtube.com/vi/SsqUDfQHbjE/0.jpg)](https://www.youtube.com/watch?v=SsqUDfQHbjE)

## Installation

Installation requires `gstreamer` for your platform.

After you install `gstreamer`, you can compile and install by cloning the repository and running `cargo install --path .`

Alternatively, you can run `RUSTFLAGS="-C target-cpu=native"  cargo install --profile=release-lto --path .` to enable architecture-specific optimizations

## FAQ

- Q: Why the hell would you want this?
  - A: You can use it to quickly preview videos in the terminal. Otherwise, it's mostly just for fun and it looks cool.
- Q: How is it so fast?
  - A: It's mostly `gstreamer` being fast, but also the renderer only renders what changes between frames and optimizes escape sequence output. Also it's fast due to making the buffer on stdout quite massive.
- Q: Why is it being slow?
  - A: Skill issue 🚀️. But seriously, if you're having performance issues, use a faster terminal emulator like [alacritty](https://github.com/alacritty/alacritty). That terminal emulator is ridiculously fast and is perfect for `onna`.
- Q: Can it play Bad Apple?
  - A: Yes, and with no dropped frames at a 319x77 terminal resolution with xterm on my machine.
- Q: Can it run DOOM?
  - A: It currently doesn't allow user interaction, so it can't play games, but it can watch DOOM gameplay.
- Q: Where does the name come from?
  - A: `onna` (`女`) in Japanese translates to `girl`. I named it this after a video I used for testing, [`女の子になりたい` (I wanna be a girl)](https://www.youtube.com/watch?v=ucbx9we6EHk) by `まふまふ` (MafuMafu).
    This was the first video I thought of other than Bad Apple and it proved to be an amazing video for testing because it has lots of changing colors, lots of different edge cases, and because I could actually tolerate watching the video over 20 times in a single day.
- Q: Why does kitty crash when I resize the window after running `onna` in kitty mode.
  - A: This is a [known kitty bug](https://github.com/kovidgoyal/kitty/issues/6555) with images. Try building kitty from source. If it still crashes, it's not my fault.
- Q: When's sixel support coming?
  - A: Soon™️
- Q: Why do you need so many questions in an FAQ?
  - A: Because people asked me a bunch of questions. Why else?
- Q: What's

## Changelog

- Version 0.2.0
  - Refactored the entire codebase to make the code make more sense
  - Optimizations :3
    - When a stride has the same color as the stride before it don't print the color's escape sequence
    - When a stride directly follows another, don't print the cursor moving escape
  - Added `background` mode, which writes images as space characters with a background color
  - Added experimental `kitty` mode which uses the kitty image protocol
  - Applied gamma correction in `ascii` mode
  - Dark pixels will no longer occasionally become way too bright for no apparent reason
  - The dropped frame counter will no longer appear in random places when interrupted with ctrl + c
  - The dropped frame counter will no longer be written to standard error when interrupted with ctrl + c
  - Anime girls will no longer try to break the fourth wall and escape the confinement of the user's screen
