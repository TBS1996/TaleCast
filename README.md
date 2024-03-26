# CringeCast

Simple CLI podcatcher.

## why?

i had a few issues that caused me to write this program.

1. Bad filenames.
I use syncthing to sync to my phone and from there i use a normal audio player. The filenames are usually unintelligble so I wanted a podcatcher that renamed the filename to the title of the episode.

2. Better control over which episodes to download.
Using other apps they always wanted me to download the entire catalogue or some arbitrary number. That's annoying. When I add a new podcast I usually just want a few episodes in the past and then to follow it from there. So in this program it's easy to write a global default of how old the episodes can be, and it can be overridden per podcast. 

3. Avoid databases.
I dislike databases for simple terminal programs. Other programs tend to use a database to save which episodes have already been downloaded. My approach is a simple textfile ".downloaded" that keeps a list of the GUID's of downloaded episodes. This means if you move files or delete them, they won't be downloaded again, unless you delete the .downloaded file or some lines within it.

