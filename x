talecast v0.1.30 (/home/tor/prog/talecast)
├── chrono v0.4.35
│   ├── iana-time-zone v0.1.60
│   └── num-traits v0.2.18
│       [build-dependencies]
│       └── autocfg v1.1.0
├── clap v4.5.4
│   ├── clap_builder v4.5.2
│   │   ├── anstream v0.6.13
│   │   │   ├── anstyle v1.0.6
│   │   │   ├── anstyle-parse v0.2.3
│   │   │   │   └── utf8parse v0.2.1
│   │   │   ├── anstyle-query v1.0.2
│   │   │   ├── colorchoice v1.0.0
│   │   │   └── utf8parse v0.2.1
│   │   ├── anstyle v1.0.6
│   │   ├── clap_lex v0.7.0
│   │   └── strsim v0.11.1
│   └── clap_derive v4.5.4 (proc-macro)
│       ├── heck v0.5.0
│       ├── proc-macro2 v1.0.79
│       │   └── unicode-ident v1.0.12
│       ├── quote v1.0.35
│       │   └── proc-macro2 v1.0.79 (*)
│       └── syn v2.0.55
│           ├── proc-macro2 v1.0.79 (*)
│           ├── quote v1.0.35 (*)
│           └── unicode-ident v1.0.12
├── dateparser v0.2.1
│   ├── anyhow v1.0.82
│   ├── chrono v0.4.35 (*)
│   ├── lazy_static v1.4.0
│   └── regex v1.10.4
│       ├── aho-corasick v1.1.3
│       │   └── memchr v2.7.1
│       ├── memchr v2.7.1
│       ├── regex-automata v0.4.6
│       │   ├── aho-corasick v1.1.3 (*)
│       │   ├── memchr v2.7.1
│       │   └── regex-syntax v0.8.3
│       └── regex-syntax v0.8.3
├── dirs v5.0.1
│   └── dirs-sys v0.4.1
│       ├── libc v0.2.153
│       └── option-ext v0.2.0
├── futures v0.3.30
│   ├── futures-channel v0.3.30
│   │   ├── futures-core v0.3.30
│   │   └── futures-sink v0.3.30
│   ├── futures-core v0.3.30
│   ├── futures-executor v0.3.30
│   │   ├── futures-core v0.3.30
│   │   ├── futures-task v0.3.30
│   │   └── futures-util v0.3.30
│   │       ├── futures-channel v0.3.30 (*)
│   │       ├── futures-core v0.3.30
│   │       ├── futures-io v0.3.30
│   │       ├── futures-macro v0.3.30 (proc-macro)
│   │       │   ├── proc-macro2 v1.0.79 (*)
│   │       │   ├── quote v1.0.35 (*)
│   │       │   └── syn v2.0.55 (*)
│   │       ├── futures-sink v0.3.30
│   │       ├── futures-task v0.3.30
│   │       ├── memchr v2.7.1
│   │       ├── pin-project-lite v0.2.13
│   │       ├── pin-utils v0.1.0
│   │       └── slab v0.4.9
│   │           [build-dependencies]
│   │           └── autocfg v1.1.0
│   ├── futures-io v0.3.30
│   ├── futures-sink v0.3.30
│   ├── futures-task v0.3.30
│   └── futures-util v0.3.30 (*)
├── futures-util v0.3.30 (*)
├── id3 v1.13.1
│   ├── bitflags v2.5.0
│   ├── byteorder v1.5.0
│   └── flate2 v1.0.28
│       ├── crc32fast v1.4.0
│       │   └── cfg-if v1.0.0
│       └── miniz_oxide v0.7.2
│           └── adler v1.0.2
├── indicatif v0.17.8
│   ├── console v0.15.8
│   │   ├── lazy_static v1.4.0
│   │   ├── libc v0.2.153
│   │   └── unicode-width v0.1.11
│   ├── number_prefix v0.4.0
│   ├── portable-atomic v1.6.0
│   └── unicode-width v0.1.11
├── mime_guess v2.0.4
│   ├── mime v0.3.17
│   └── unicase v2.7.0
│       [build-dependencies]
│       └── version_check v0.9.4
│   [build-dependencies]
│   └── unicase v2.7.0 (*)
├── opml v1.1.6
│   ├── hard-xml v1.36.0
│   │   ├── hard-xml-derive v1.36.0 (proc-macro)
│   │   │   ├── bitflags v2.5.0
│   │   │   ├── proc-macro2 v1.0.79 (*)
│   │   │   ├── quote v1.0.35 (*)
│   │   │   └── syn v1.0.109
│   │   │       ├── proc-macro2 v1.0.79 (*)
│   │   │       ├── quote v1.0.35 (*)
│   │   │       └── unicode-ident v1.0.12
│   │   ├── jetscii v0.5.3
│   │   ├── lazy_static v1.4.0
│   │   ├── memchr v2.7.1
│   │   └── xmlparser v0.13.6
│   ├── serde v1.0.197
│   │   └── serde_derive v1.0.197 (proc-macro)
│   │       ├── proc-macro2 v1.0.79 (*)
│   │       ├── quote v1.0.35 (*)
│   │       └── syn v2.0.55 (*)
│   └── thiserror v1.0.58
│       └── thiserror-impl v1.0.58 (proc-macro)
│           ├── proc-macro2 v1.0.79 (*)
│           ├── quote v1.0.35 (*)
│           └── syn v2.0.55 (*)
├── percent-encoding v2.3.1
├── quick-xml v0.31.0
│   └── memchr v2.7.1
├── quickxml_to_serde v0.6.0
│   ├── minidom v0.12.0
│   │   └── quick-xml v0.17.2
│   │       └── memchr v2.7.1
│   ├── regex v1.10.4 (*)
│   ├── serde v1.0.197 (*)
│   ├── serde_derive v1.0.197 (proc-macro) (*)
│   └── serde_json v1.0.115
│       ├── itoa v1.0.10
│       ├── ryu v1.0.17
│       └── serde v1.0.197 (*)
├── regex v1.10.4 (*)
├── reqwest v0.12.2
│   ├── base64 v0.21.7
│   ├── bytes v1.6.0
│   ├── encoding_rs v0.8.33
│   │   └── cfg-if v1.0.0
│   ├── futures-core v0.3.30
│   ├── futures-util v0.3.30 (*)
│   ├── h2 v0.4.3
│   │   ├── bytes v1.6.0
│   │   ├── fnv v1.0.7
│   │   ├── futures-core v0.3.30
│   │   ├── futures-sink v0.3.30
│   │   ├── futures-util v0.3.30 (*)
│   │   ├── http v1.1.0
│   │   │   ├── bytes v1.6.0
│   │   │   ├── fnv v1.0.7
│   │   │   └── itoa v1.0.10
│   │   ├── indexmap v2.2.6
│   │   │   ├── equivalent v1.0.1
│   │   │   └── hashbrown v0.14.3
│   │   ├── slab v0.4.9 (*)
│   │   ├── tokio v1.36.0
│   │   │   ├── bytes v1.6.0
│   │   │   ├── libc v0.2.153
│   │   │   ├── mio v0.8.11
│   │   │   │   └── libc v0.2.153
│   │   │   ├── num_cpus v1.16.0
│   │   │   │   └── libc v0.2.153
│   │   │   ├── parking_lot v0.12.1
│   │   │   │   ├── lock_api v0.4.11
│   │   │   │   │   └── scopeguard v1.2.0
│   │   │   │   │   [build-dependencies]
│   │   │   │   │   └── autocfg v1.1.0
│   │   │   │   └── parking_lot_core v0.9.9
│   │   │   │       ├── cfg-if v1.0.0
│   │   │   │       ├── libc v0.2.153
│   │   │   │       └── smallvec v1.13.2
│   │   │   ├── pin-project-lite v0.2.13
│   │   │   ├── signal-hook-registry v1.4.1
│   │   │   │   └── libc v0.2.153
│   │   │   ├── socket2 v0.5.6
│   │   │   │   └── libc v0.2.153
│   │   │   └── tokio-macros v2.2.0 (proc-macro)
│   │   │       ├── proc-macro2 v1.0.79 (*)
│   │   │       ├── quote v1.0.35 (*)
│   │   │       └── syn v2.0.55 (*)
│   │   ├── tokio-util v0.7.10
│   │   │   ├── bytes v1.6.0
│   │   │   ├── futures-core v0.3.30
│   │   │   ├── futures-sink v0.3.30
│   │   │   ├── pin-project-lite v0.2.13
│   │   │   ├── tokio v1.36.0 (*)
│   │   │   └── tracing v0.1.40
│   │   │       ├── log v0.4.21
│   │   │       ├── pin-project-lite v0.2.13
│   │   │       └── tracing-core v0.1.32
│   │   │           └── once_cell v1.19.0
│   │   └── tracing v0.1.40 (*)
│   ├── http v1.1.0 (*)
│   ├── http-body v1.0.0
│   │   ├── bytes v1.6.0
│   │   └── http v1.1.0 (*)
│   ├── http-body-util v0.1.1
│   │   ├── bytes v1.6.0
│   │   ├── futures-core v0.3.30
│   │   ├── http v1.1.0 (*)
│   │   ├── http-body v1.0.0 (*)
│   │   └── pin-project-lite v0.2.13
│   ├── hyper v1.2.0
│   │   ├── bytes v1.6.0
│   │   ├── futures-channel v0.3.30 (*)
│   │   ├── futures-util v0.3.30 (*)
│   │   ├── h2 v0.4.3 (*)
│   │   ├── http v1.1.0 (*)
│   │   ├── http-body v1.0.0 (*)
│   │   ├── httparse v1.8.0
│   │   ├── itoa v1.0.10
│   │   ├── pin-project-lite v0.2.13
│   │   ├── smallvec v1.13.2
│   │   ├── tokio v1.36.0 (*)
│   │   └── want v0.3.1
│   │       └── try-lock v0.2.5
│   ├── hyper-tls v0.6.0
│   │   ├── bytes v1.6.0
│   │   ├── http-body-util v0.1.1 (*)
│   │   ├── hyper v1.2.0 (*)
│   │   ├── hyper-util v0.1.3
│   │   │   ├── bytes v1.6.0
│   │   │   ├── futures-channel v0.3.30 (*)
│   │   │   ├── futures-util v0.3.30 (*)
│   │   │   ├── http v1.1.0 (*)
│   │   │   ├── http-body v1.0.0 (*)
│   │   │   ├── hyper v1.2.0 (*)
│   │   │   ├── pin-project-lite v0.2.13
│   │   │   ├── socket2 v0.5.6 (*)
│   │   │   ├── tokio v1.36.0 (*)
│   │   │   ├── tower v0.4.13
│   │   │   │   ├── futures-core v0.3.30
│   │   │   │   ├── futures-util v0.3.30 (*)
│   │   │   │   ├── pin-project v1.1.5
│   │   │   │   │   └── pin-project-internal v1.1.5 (proc-macro)
│   │   │   │   │       ├── proc-macro2 v1.0.79 (*)
│   │   │   │   │       ├── quote v1.0.35 (*)
│   │   │   │   │       └── syn v2.0.55 (*)
│   │   │   │   ├── pin-project-lite v0.2.13
│   │   │   │   ├── tokio v1.36.0 (*)
│   │   │   │   ├── tower-layer v0.3.2
│   │   │   │   ├── tower-service v0.3.2
│   │   │   │   └── tracing v0.1.40 (*)
│   │   │   ├── tower-service v0.3.2
│   │   │   └── tracing v0.1.40 (*)
│   │   ├── native-tls v0.2.11
│   │   │   ├── log v0.4.21
│   │   │   ├── openssl v0.10.64
│   │   │   │   ├── bitflags v2.5.0
│   │   │   │   ├── cfg-if v1.0.0
│   │   │   │   ├── foreign-types v0.3.2
│   │   │   │   │   └── foreign-types-shared v0.1.1
│   │   │   │   ├── libc v0.2.153
│   │   │   │   ├── once_cell v1.19.0
│   │   │   │   ├── openssl-macros v0.1.1 (proc-macro)
│   │   │   │   │   ├── proc-macro2 v1.0.79 (*)
│   │   │   │   │   ├── quote v1.0.35 (*)
│   │   │   │   │   └── syn v2.0.55 (*)
│   │   │   │   └── openssl-sys v0.9.101
│   │   │   │       └── libc v0.2.153
│   │   │   │       [build-dependencies]
│   │   │   │       ├── cc v1.0.90
│   │   │   │       ├── pkg-config v0.3.30
│   │   │   │       └── vcpkg v0.2.15
│   │   │   ├── openssl-probe v0.1.5
│   │   │   └── openssl-sys v0.9.101 (*)
│   │   ├── tokio v1.36.0 (*)
│   │   ├── tokio-native-tls v0.3.1
│   │   │   ├── native-tls v0.2.11 (*)
│   │   │   └── tokio v1.36.0 (*)
│   │   └── tower-service v0.3.2
│   ├── hyper-util v0.1.3 (*)
│   ├── ipnet v2.9.0
│   ├── log v0.4.21
│   ├── mime v0.3.17
│   ├── native-tls v0.2.11 (*)
│   ├── once_cell v1.19.0
│   ├── percent-encoding v2.3.1
│   ├── pin-project-lite v0.2.13
│   ├── rustls-pemfile v1.0.4
│   │   └── base64 v0.21.7
│   ├── serde v1.0.197 (*)
│   ├── serde_urlencoded v0.7.1
│   │   ├── form_urlencoded v1.2.1
│   │   │   └── percent-encoding v2.3.1
│   │   ├── itoa v1.0.10
│   │   ├── ryu v1.0.17
│   │   └── serde v1.0.197 (*)
│   ├── sync_wrapper v0.1.2
│   ├── tokio v1.36.0 (*)
│   ├── tokio-native-tls v0.3.1 (*)
│   ├── tokio-util v0.7.10 (*)
│   ├── tower-service v0.3.2
│   └── url v2.5.0
│       ├── form_urlencoded v1.2.1 (*)
│       ├── idna v0.5.0
│       │   ├── unicode-bidi v0.3.15
│       │   └── unicode-normalization v0.1.23
│       │       └── tinyvec v1.6.0
│       │           └── tinyvec_macros v0.1.1
│       └── percent-encoding v2.3.1
├── rss v2.0.7
│   ├── atom_syndication v0.12.2
│   │   ├── chrono v0.4.35 (*)
│   │   ├── derive_builder v0.12.0
│   │   │   └── derive_builder_macro v0.12.0 (proc-macro)
│   │   │       ├── derive_builder_core v0.12.0
│   │   │       │   ├── darling v0.14.4
│   │   │       │   │   ├── darling_core v0.14.4
│   │   │       │   │   │   ├── fnv v1.0.7
│   │   │       │   │   │   ├── ident_case v1.0.1
│   │   │       │   │   │   ├── proc-macro2 v1.0.79 (*)
│   │   │       │   │   │   ├── quote v1.0.35 (*)
│   │   │       │   │   │   ├── strsim v0.10.0
│   │   │       │   │   │   └── syn v1.0.109 (*)
│   │   │       │   │   └── darling_macro v0.14.4 (proc-macro)
│   │   │       │   │       ├── darling_core v0.14.4 (*)
│   │   │       │   │       ├── quote v1.0.35 (*)
│   │   │       │   │       └── syn v1.0.109 (*)
│   │   │       │   ├── proc-macro2 v1.0.79 (*)
│   │   │       │   ├── quote v1.0.35 (*)
│   │   │       │   └── syn v1.0.109 (*)
│   │   │       └── syn v1.0.109 (*)
│   │   ├── diligent-date-parser v0.1.4
│   │   │   └── chrono v0.4.35 (*)
│   │   ├── never v0.1.0
│   │   └── quick-xml v0.30.0
│   │       ├── encoding_rs v0.8.33 (*)
│   │       └── memchr v2.7.1
│   ├── derive_builder v0.12.0 (*)
│   ├── never v0.1.0
│   └── quick-xml v0.30.0 (*)
├── sanitize-filename v0.5.0
│   ├── lazy_static v1.4.0
│   └── regex v1.10.4 (*)
├── serde v1.0.197 (*)
├── serde_json v1.0.115 (*)
├── strum v0.21.0
├── strum_macros v0.21.1 (proc-macro)
│   ├── heck v0.3.3
│   │   └── unicode-segmentation v1.11.0
│   ├── proc-macro2 v1.0.79 (*)
│   ├── quote v1.0.35 (*)
│   └── syn v1.0.109 (*)
├── tokio v1.36.0 (*)
├── toml v0.5.11
│   └── serde v1.0.197 (*)
├── unicode-width v0.1.11
└── uuid v1.8.0
