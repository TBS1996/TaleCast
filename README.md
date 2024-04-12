# TaleCast

Simple CLI podcast manager.

Check this video for a quick introduction:
[![Watch the video](https://img.youtube.com/vi/TKoToA6MGdY/0.jpg)](https://www.youtube.com/watch?v=TKoToA6MGdY)

## Main features

- Easy to configure which episodes to be downloaded
- Mp3 tags normalization
- Granular configuration control of each podcast
- Backlog mode to catch up on old episodes at your own pace
- Download hook for post-download processing
- OPML export
- OPML import
- Git-friendly download-tracker (textfile where 1 episode == 1 line)
- Advanced pattern-matching for naming your files (and more!)
- Set Custom ip3v2 tags
- Parallel downloads
- Partial download support
- Downloaded paths can be printed to stdout for easy piping
- Pretty graphics
   

## how to install?

You'll need to have rust installed. Either download from cargo `cargo install talecast` or just clone the repo.
I plan to put it on the nix store soon, not sure if I'm gonna bother with the other package managers sinceim less familiar. If someone wants to publish there then that'd be great!

## how to configure it?

the global config is located in:
`~/.config/talecast/config.toml`

you put your podcasts in this file:
~/.config/talecast/podcasts.toml`

## how to add podcasts?

`talecast --add $PODCAST_URL $PODCAST_NAME`

or modify the `podcasts.toml` file directly. 

Check out the video for more details. But more documentation to come!
