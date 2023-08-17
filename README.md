# Onna - A real-time video player / streamer for the terminal

## Features

- Support for as many video formats as your `gstreamer` installation supports
- Supports any terminal that supports ANSI escape sequences for cursor movement and color

  - Truecolor and ANSI 256 color
- Real-time video playing / streaming with audio
- Frame dropping when the renderer gets out of sync

## Previews

No previews yet because my laptop is being a potato rn.

## Installation

Installation requires `gstreamer` for your platform.

After you install `gstreamer`, you can compile and install by cloning the repository and running `cargo install --path .`

## FAQ

- Q: Why the hell would you want this?
  - A: You can use it to quickly preview videos in the terminal. Otherwise, it's mostly just for fun and it looks cool
- Q: How is it so fast?
  - A: It's mostly `gstreamer` being fast, but also the renderer only renders what changes between frames and optimizes escape sequence output. Also it's fast due to making the buffer on stdout quite massive.
- Q: Why is it being slow?
  - A: Skill issue 🚀️
- Q: Can it play Bad Apple?
  - A: Yes, and with no dropped frames at a 319x77 terminal resolution on my machine.
- Q: Can it run DOOM?
  - A: It currently doesn't allow user interaction, so it can't play games, but it can watch DOOM gameplay.
- Q: Where does the name come from?
  - A: `onna` (`女`) in Japanese translates to `girl`. I named it this after a video I used for testing, [`女の子になりたい` (I wanna be a girl)](https://www.youtube.com/watch?v=ucbx9we6EHk) by `まふまふ` (MafuMafu).
    This was the first video I thought of other than Bad Apple and it proved to be an amazing video for testing because it has lots of changing colors, lots of different edge cases, and because I could actually tolerate watching the video over 20 times in a single day.
- Q: Why do you need so many questions in an FAQ?
  - A: Because people asked me a bunch of questions. Why else?
