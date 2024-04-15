# TaleCast

Simple CLI podcast manager.



![demo](https://github.com/TBS1996/TaleCast/assets/56874491/4eb96b52-6752-4280-84b6-306be6c9ab84)



Check this video for a quick tutorial:  
[![Watch the video](https://img.youtube.com/vi/TKoToA6MGdY/0.jpg)](https://www.youtube.com/watch?v=TKoToA6MGdY)

If you want to sync with your phone you could consider using syncthing. 

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
- Filter which episodes to sync or export with regex patterns
   

## how to install?

You'll need to have rust installed.   
Either download from cargo with `cargo install talecast` or just clone the repo and do `cargo run`.  
  
I plan to put it on the nix store soon, not sure if I'm gonna bother with the other package managers since I'm less familiar. If someone wants to publish there then that'd be great!


## how to add podcasts?

`talecast --add $PODCAST_URL $PODCAST_NAME`  
... or modify the `podcasts.toml` file directly. 

for finding podcast urls, I recommend this website: https://podcastindex.org/   
on the page of a given podcast there, click 'copy rss'. This is the url you should use! 


## how to configure it?

the global config is located in:
`~/.config/talecast/config.toml`

you put your podcasts in this file:
`~/.config/talecast/podcasts.toml`


## what are the config options?

The way configuration works is that you can set a 'global value' that applies to all podcasts in the `config.toml` file, however, you can override them by 
setting the same setting under a given podcast in the `podcasts.toml` file. If a value is not required, you can have it configured globally but disable it on 
specific podcasts with "$SETTING = false".

| setting          | description                                                  | required | per-podcast | global | default                                     |
|------------------|--------------------------------------------------------------|----------|-------------|--------|---------------------------------------------|
| url              | the url to the xml file of the podcast                       | yes      | ✅           | ❌      | no default, must be specified               |
| download_path    | the path where episodes will be downloaded                   | yes      | ✅           | ✅      | `{home}/{appname}/{podname}`                |
| name_pattern     | pattern determining name of episode files                    | yes      | ✅           | ✅      | `{pubdate::%Y-%m-%d} {rss::episode::title}` |
| id_pattern       | episode ID for determining if an episode has been downloaded | yes      | ✅           | ✅      | `{guid}`                                    |
| download_hook    | path to script that will run after an episode is downloaded  | no       | ✅           | ✅      | `None`                                      |
| tracker_path     | path to textfile that tracks downloaded episodes.            | no       | ✅           | ✅      | download_path/.downlaoded                   |
| max_days         | episodes older than this won't be downloaded                 | no       | ✅           | ✅      | `None`                                      |
| max_episodes     | only this amount of episodes from past will be downloaded    | no       | ✅           | ✅      | `None`                                      |
| earliest_date    | episodes published before this won't be downloaded           | no       | ✅           | ✅      | `None`                                      |
| id3_tags         | custom tags that mp3 files will be annotated with            | no       | ✅           | ✅      | `[ ]`                                       |
| backlog_start    | start date of when backlog mode calculates from              | no       | ✅           | ❌      | `None`                                      |
| backlog_interval | how many days pass between each new episode in backlog mode  | no       | ✅           | ❌      | `None`                                      |

## what are these weird curly brace patterns?

it's just a way to generate some dynamic texts. theres two types, unit patterns that take no input, and data patterns where you give it an input. here's the unit ones:

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


# todo  

- better error handling. Atm i unwrap a lot since stopping the program when something goes wrong is generally fine for scripts and unwrap gives a lot of nice debug information.
- integrate opml better. Currently if you import opml and then export you might lose some metadata. 
- add to package managers (help appreciated here!) 
- more tests
- maybe make it more generalizable for other kind of media content?
- atom support? do any podcasts even use atom?
- reduce dependencies
- more flexibility in how to handle missing values in patterns
