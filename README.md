# KRust

`KRust` is a simple Apache Kafka GUI client useful for developers as well adminstrators,
built using GTK and [Relm4].


The topics messages are visualized using tabs, which enables quick
navigation throughout multiple topics.

`KRust` is in the early stages of development. Manipulating important topics with it
risks data loss.

## Platform support

Development is currently focused on Linux, but bug reports for other platforms
are welcome.

The application is known to run successfully on Microsoft Windows 10/11. Other platforms are
untested, but if you can get the system dependencies to build, `KRust` should work.

## Hacking

`KRust` is a Rust project that utilizes [GTK 4][install-gtk],
[GtkSourceView][install-gtksourceview], and [libadwaita][install-libadwaita].

1. First, [install Rust and Cargo][install-rust].

2. Install system dependencies.

    #### Arch Linux

    ```sh
    $ pacman -Syu gtk4 libadwaita gtksourceview5 libsasl openssl
    ```

    #### Fedora

    ```
    $ dnf install -y gtk4 libadwaita-devel cyrus-sasl-devel openssl-devel
    ```

    #### Ubuntu

    ```sh
    $ apt install libsasl2-2 libgtksourceview-5-0 libadwaita-1-0 libssl3t64
    ```

3. Build and run the application.

    ```sh
    $ cargo run
    ```

## License

`KRust` is licensed under the GPLv3 license (see [COPYING](COPYING) or [GPLv3](https://www.gnu.org/licenses/gpl-3.0.txt)).

[install-rust]: https://rustup.rs/
[install-gtk]: https://www.gtk.org/docs/installations/
[install-gtksourceview]: https://wiki.gnome.org/Projects/GtkSourceView
[install-libadwaita]: https://gnome.pages.gitlab.gnome.org/libadwaita/
[Relm4]: https://aaronerhardt.github.io/relm4-book/book/
