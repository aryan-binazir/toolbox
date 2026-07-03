# Install without config overwrite

AI Scheduler includes a Makefile for local install and update. `make install` builds and installs the app binary and desktop launcher but does not overwrite `config.toml`, routines, or the SQLite run history. Config bootstrapping is an explicit separate target that only copies the example config when no local config exists.
