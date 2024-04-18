# TaleCast

TaleCast is a simple and powerful CLI podcast manager that makes it easy to search, add, and manage your favorite podcasts directly from the terminal.

![Demo](https://github.com/TBS1996/TaleCast/assets/56874491/4eb96b52-6752-4280-84b6-306be6c9ab84)

## Features

- Search and add podcasts directly from the terminal
- Configurable episode downloading options
- MP3 tag normalization
- Granular configuration control for each podcast
- Backlog mode to catch up on old episodes at your own pace
- Download hook for post-download processing
- OPML export and import
- Git-friendly download-tracker (textfile where 1 episode == 1 line)
- Advanced pattern-matching for naming files and more
- Custom ID3v2 tag support
- Parallel downloads
- Partial download support
- Ability to print downloaded paths to stdout for easy piping
- Pretty graphics
- Filter episodes to sync or export using regex patterns
- Built-in symlink support

## Installation

### Using Cargo

To install TaleCast using Cargo, you'll need to have Rust installed. If you haven't used Rust before, run the shell command from the official website: [https://www.rust-lang.org/learn/get-started](https://www.rust-lang.org/learn/get-started)

Once Rust is installed, you can install TaleCast with the following command:

```bash
cargo install talecast
```

### Arch Linux (AUR)

TaleCast is available in the [Arch User Repository (AUR)](https://aur.archlinux.org/packages/talecast-git) for Arch Linux users. You can install it using your preferred AUR helper, such as `paru` or `yay`.

To install TaleCast with `paru`, run the following command:

```bash
paru -S talecast-git
```

### Other Package Managers

If you have experience packaging for a package manager not listed here, it would be greatly appreciated if you add it and let me know about it!

## Usage

### Adding Podcasts

There are several ways to add podcasts to TaleCast:

- Search for podcasts with `talecast --search $NAME`
- Add a podcast directly with `talecast --add $PODCAST_URL $PODCAST_NAME`
- Edit the `podcasts.toml` file directly (see the 'Configuration' section below)

For finding podcast URLs, I recommend using [https://podcastindex.org/](https://podcastindex.org/). On the page of a given podcast, click 'copy rss' to get the URL you should use.

If you add podcasts from the command line, you can combine it with the `catch-up` argument to only download upcoming episodes. For example: `talecast -cs "this american life"`.

### Command Line Options

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

### Configuration

To edit the global config, run `talecast --edit-config`.
To edit the podcasts, run `talecast --edit-podcasts`.

These files are located in `~/.config/talecast/config.toml` and `~/.config/talecast/podcasts.toml` respectively, unless your `XDG_CONFIG_HOME` environment variable is set to something else.

The way configuration works is that you can set a 'global value' that applies to all podcasts in the `config.toml` file. However, you can override these settings by specifying the same setting under a given podcast in the `podcasts.toml` file. If a value is not required, you can have it configured globally but disable it on specific podcasts with `$SETTING = false`.

| Setting          | Description                                                  | Required | Per-Podcast | Global | Default                                       |
| ---------------- | ------------------------------------------------------------ | -------- | ----------- | ------ | --------------------------------------------- |
| url              | The URL to the XML file of the podcast                       | Yes      | ✅          | ❌     | No default, must be specified                 |
| download_path    | The path where episodes will be downloaded                   | Yes      | ✅          | ✅     | `"{home}/talecast/{podname}"`                 |
| name_pattern     | Pattern determining the name of episode files                | Yes      | ✅          | ✅     | `"{pubdate::%Y-%m-%d} {rss::episode::title}"` |
| id_pattern       | Episode ID for determining if an episode has been downloaded | Yes      | ✅          | ✅     | `"{guid}"`                                    |
| download_hook    | Path to script that will run after an episode is downloaded  | No       | ✅          | ✅     | `None`                                        |
| tracker_path     | Path to textfile that tracks downloaded episodes             | No       | ✅          | ✅     | `download_path/.downloaded`                   |
| max_days         | Episodes older than this won't be downloaded                 | No       | ✅          | ✅     | `None`                                        |
| max_episodes     | Only this number of past episodes will be downloaded         | No       | ✅          | ✅     | `None`                                        |
| earliest_date    | Episodes published before this date won't be downloaded      | No       | ✅          | ✅     | `None`                                        |
| id3_tags         | Custom tags that MP3 files will be annotated with            | No       | ✅          | ✅     | `[]`                                          |
| symlink          | Directory where downloaded files will be symlinked to        | No       | ✅          | ✅     | `None`                                        |
| backlog_start    | Start date of when backlog mode calculates from              | No       | ✅          | ❌     | `None`                                        |
| backlog_interval | How many days pass between each new episode in backlog mode  | No       | ✅          | ❌     | `None`                                        |

### Pattern System

TaleCast provides a way to generate dynamic text using a pattern system. There are two types of patterns: unit patterns that take no input, and data patterns where you provide an input.

Unit Patterns:

| Pattern | Evaluates to                       |
| ------- | ---------------------------------- |
| guid    | The GUID of an episode             |
| url     | The URL to the episode's enclosure |
| podname | Configured name of the podcast     |
| home    | The path to your home directory    |

A good example of these is the default value of the `download_path` setting.

Data Patterns:

| Pattern      | Description                                                                                                                          |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------ |
| rss::episode | Represents the XML of an individual episode. The data it takes in is the name of an XML tag. The output is the contents of that tag. |
| rss::channel | Represents the XML of a podcast. The data it takes in is the name of an XML tag. The output is the contents of that tag.             |
| pubdate      | The time the episode was published. Takes in a formatter string.                                                                     |

Look at the default value of the `name_pattern` setting for an example of how to use them.

Note that not all patterns are available for each setting. For example, the `download_path` can't use information specific to an episode.

### Backlog Mode

Backlog mode is a way to systematically go through the backlog of a podcast, starting from the first episode. It's perfect for podcasts where older episodes are as relevant as newer ones, and especially if you're supposed to go through them chronologically.

To use backlog mode, set the `backlog_start` date and then sync. TaleCast will download the first episode of the podcast. After `backlog_interval` days have passed, it will download the second episode, and so on.

## Contributing

If you encounter any bugs or have feature requests, please use the GitHub issue page. If you're reporting a bug, make sure you have the latest version of TaleCast in case it has already been fixed.

Contributions are welcome! If you'd like to contribute to TaleCast, please follow these steps:

1. Fork the repository
2. Create a new branch for your feature or bug fix
3. Make your changes and commit them with descriptive commit messages
4. Push your changes to your forked repository
5. Submit a pull request to the main repository

## License

TaleCast is released under the [MIT License](LICENSE).
