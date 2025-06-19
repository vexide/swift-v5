- [ ] Add some way to detect if extraction was cancelled

    This stems from the issue that extraction is non-atomic.
    Probably this involves having a file (ex. state.json) which stores a list of correctly-installed toolchains
    instead of checking whether the toolchain folder exists to determine installation.

- [ ] Symlink toolchain into project directory

    Very much necessary for actually getting work done.
