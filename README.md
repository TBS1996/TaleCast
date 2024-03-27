# CringeCast

Simple CLI podcatcher.

<video src='https://github.com/TBS1996/kjattify/raw/main/cringecast.mp4' />

## why?

i had a few issues that caused me to write this program.

1. Bad filenames.
I use syncthing to sync to my phone and from there i use a normal audio player. The filenames are usually unintelligble so I wanted a podcatcher that renamed the filename to the title of the episode.

2. Better control over which episodes to download.
Using other apps they always wanted me to download the entire catalogue or some arbitrary number. That's annoying. When I add a new podcast I usually just want a few episodes in the past and then to follow it from there. So in this program it's easy to write a global default of how old the episodes can be, and it can be overridden per podcast. 

3. Avoid databases.
I dislike databases for simple terminal programs. Other programs tend to use a database to save which episodes have already been downloaded. My approach is a simple textfile ".downloaded" that keeps a list of the GUID's of downloaded episodes. This means if you move files or delete them, they won't be downloaded again, unless you delete the .downloaded file or some lines within it.

## how to install?

you gotta have rust installed atm. You can do `cargo install cringecast` or clone the repo and run it.

## how to configure it?

the global config is located in:
`~/.config/cringecast/config.toml`

you put your podcasts in this file:
~/.config/cringecast/podcasts.toml`

example podcasts.toml:

```toml
[freakonomics]
url="https://feeds.simplecast.com/Y8lFbOT4"

[aftenpodden]
url="https://podcast.stream.schibsted.media/ap/100168?podcast"
```

## how do i...?

- set default max episodes to download to 20, but disable the cap for a specific podcast?


config.toml:
```toml 
max_episodes=20
```

podcasts.toml:
```toml 
[freakonomics] # since no max_episodes value is set, it will use the one in config.toml.
url="https://feeds.simplecast.com/Y8lFbOT4" 

[rest_is_history]
url="https://feeds.megaphone.fm/GLT4787413333"
max_episodes=false
```

- set a limit for how old an episode is before i download?


```toml 
[freakonomics] 
url="https://feeds.simplecast.com/Y8lFbOT4" 
max_days=30 # default value can be chosen in config.toml
```

- start from the beginning and be served an episode from the backlog every 5 days?

```toml
[rest_is_history]
url="https://feeds.megaphone.fm/GLT4787413333"
backlog_start="2024-03-27" # you put the current date in the backlog_start. 
backlog_interval=5
```

- only download episodes published after 5. november 2023?

```toml
[rest_is_history]
url="https://feeds.megaphone.fm/GLT4787413333"
earliest_date="2023-11-03"
```

- change the location of where episodes are downloaded?

config.toml:
```toml
path="/foo/bar/baz"
```

- re-download episodes that have already been downloaded?

delete or modify the `.downloaded` file in the folder where the episodes are downloaded.

## why the name?

'broadcast' in norwegian means kringkasting. I thought the "kasting" sounds the 'cast' in 'podcast'. while the 'kring' part sounds like cringe. so, cringecast. pretty dumb i know.




