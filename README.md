# TaleCast

Simple CLI podcast manager.



![demo](https://github.com/TBS1996/TaleCast/assets/56874491/4eb96b52-6752-4280-84b6-306be6c9ab84)



Check this video for a quick tutorial: (although this README is more up to date)  
[![Watch the video](https://img.youtube.com/vi/TKoToA6MGdY/0.jpg)](https://www.youtube.com/watch?v=TKoToA6MGdY)

If you want to sync with your phone you could consider using syncthing. 

## Main features

- Search and add podcasts directly from the terminal
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
- Filter which episodes to sync or export with regex patterns
- Built-in symlink support

## Installation

### Using Cargo

You'll need to have Rust installed. You can get Talecast with `cargo install talecast`. If you haven't used Rust before, just run the shell command from the official website: [https://www.rust-lang.org/learn/get-started](https://www.rust-lang.org/learn/get-started)

After this, you should be able to do:

```bash
cargo install talecast
```

### Arch Linux (AUR)

Talecast is available in the [Arch User Repository (AUR)](https://aur.archlinux.org/packages/talecast-git) for Arch Linux users. You can install it using your preferred AUR helper, such as `paru` or `yay`.

To install Talecast with `paru`, run the following command:

```bash
paru -S talecast-git
```

### Other Package Managers

If you have experience packaging for a package manager not listed here, it would be greatly appreciated if you add it and let me know about it!

## Adding podcasts

Some different methods:;

- Search for podcasts with `talecast --search $NAME`  
- `talecast --add $PODCAST_URL $PODCAST_NAME`  
- Edit the `podcasts.toml` directly. See the 'how to configure' section below.

for finding podcast urls, I recommend this website: https://podcastindex.org/   
on the page of a given podcast there, click 'copy rss'. This is the url you should use! 

If you add podcasts from commandline, you can combine it with the `catch-up` argument to only download upcoming episodes.

for example: `talecast -cs this american life`.

## Commandline options

```
  -i, --import <FILE>      Import podcasts from an OPML file
  -e, --export <FILE>      Export your podcasts to an OPML file
  -p, --print              Print the downloaded paths to stdout
  -c, --catch-up           Configure to skip episodes published prior to current time. Can be combined with filter, add, and import
  -a, --add <URL> <NAME>   Add new podcast
  -f, --filter <FILTER>    Filter which podcasts to sync or export with a regex pattern
      --config <FILE>      Override the path to the config file
      --edit-config        Edit the config.toml file
      --edit-podcasts      Edit the podcasts.toml file
  -s, --search <QUERY>...  Search for podcasts to add
  -h, --help               Print help
  -V, --version            Print version
```


## Configuration

to edit the global config: `talecast --edit-config`  
to edit the podcasts: `talecast --edit-podcasts`  

these files are located in `~/.config/talecast/config.toml` and `~/.config/talecast/podcasts.toml` respectively, unless your `XDG_CONFIG_HOME` environment variable is set to something else.

The way configuration works is that you can set a 'global value' that applies to all podcasts in the `config.toml` file, however, you can override them by 
setting the same setting under a given podcast in the `podcasts.toml` file. If a value is not required, you can have it configured globally but disable it on 
specific podcasts with "$SETTING = false".

| setting          | description                                                  | required | per-podcast | global | default                                         |
|------------------|--------------------------------------------------------------|----------|-------------|--------|-------------------------------------------------|
| url              | the url to the xml file of the podcast                       | yes      | ✅           | ❌      | no default, must be specified                 |
| download_path    | the path where episodes will be downloaded                   | yes      | ✅           | ✅      | `"{home}/talecast/{podname}"`                 |
| name_pattern     | pattern determining name of episode files                    | yes      | ✅           | ✅      | `"{pubdate::%Y-%m-%d} {rss::episode::title}"` |
| id_pattern       | episode ID for determining if an episode has been downloaded | yes      | ✅           | ✅      | `"{guid}"`                                    |
| download_hook    | path to script that will run after an episode is downloaded  | no       | ✅           | ✅      | `None`                                        |
| tracker_path     | path to textfile that tracks downloaded episodes.            | no       | ✅           | ✅      | download_path/.downloaded                     |
| max_days         | episodes older than this won't be downloaded                 | no       | ✅           | ✅      | `None`                                        |
| max_episodes     | only this amount of episodes from past will be downloaded    | no       | ✅           | ✅      | `None`                                        |
| earliest_date    | episodes published before this won't be downloaded           | no       | ✅           | ✅      | `None`                                        |
| id3_tags         | custom tags that mp3 files will be annotated with            | no       | ✅           | ✅      | `[ ]`                                         |
| symlink          | directory where downloaded files will be symlinked to        | no       | ✅           | ✅      | `None`                                        |
| backlog_start    | start date of when backlog mode calculates from              | no       | ✅           | ❌      | `None`                                        |
| backlog_interval | how many days pass between each new episode in backlog mode  | no       | ✅           | ❌      | `None`                                        |

## Pattern system 

A way to generate some dynamic texts. theres two types, unit patterns that take no input, and data patterns where you give it an input. here's the unit ones:

| pattern | evalutes to..                      |
|---------|------------------------------------|
| guid    | the guid of an episode             |
| url     | the url to the episode's enclosure |
| podname | configured name of the podcast     |
| home    | the path to your home directory    |   

 a good example of these is the default value of the `download_path` setting. 

 the following are patterns that take in an argument:

| pattern      | description                                                                                                                         |
|--------------|-------------------------------------------------------------------------------------------------------------------------------------|
| rss::episode | represents the xml of an individual episode. the data it takes in is the name of an xml tag. the output is the contents of that tag |
| rss::channel | represents the xml of a podcast. the data it takes in is the name of an xml tag. the output is the contents of that tag             |
| pubdate      | the time the episode was published. Takes in a formatter string                                                                     |
| id3tag       | takes in the name of an id3v2 tag, outputs the contents of the tag. Valid for mp3 files.                                            |


look at the default value of the name_pattern setting for an example of how to use them. 
note that not all patterns are available for each setting, for example, the download_path can't use information specific to an episode.

## Backlog mode

A way to systematically go through the backlog of a podacst, starting from the first episode. Perfect for podcasts where older videos are as relevant as newer ones, and especially if you're supposed to go through them chronologically. You set the date you start with `backlog_start`, and an interval with `backlog_interval`. If you set `backlog_start` and then sync, you'll download the first episode of the podcast. After `backlog_interval`` days have passed, it'll download the second episode, and so on.

## Bugs and feature requests

If you have any feedback just use the github issue page! If it's a bug, make sure you have the latest version in case I've already fixed it.

## Todo  

- better error handling. Atm i unwrap a lot since stopping the program when something goes wrong is generally fine for scripts and unwrap gives a lot of nice debug information.
- integrate opml better. Currently if you import opml and then export you might lose some metadata. 
- add to more package managers (help appreciated here!) 
- more tests
- maybe make it more generalizable for other kind of media content?
- atom support? do any podcasts even use atom?
- reduce dependencies
- more flexibility in how to handle missing values in patterns
