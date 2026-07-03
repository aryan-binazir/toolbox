PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin
APPDIR ?= $(or $(XDG_DATA_HOME),$(HOME)/.local/share)/applications
CONFIG_DIR ?= $(or $(XDG_CONFIG_HOME),$(HOME)/.config)/ai-scheduler
BIN := src-tauri/target/release/ai-scheduler

.PHONY: install update build test dev bootstrap-config clean

install: build
	install -d "$(BINDIR)"
	install -m 0755 "$(BIN)" "$(BINDIR)/ai-scheduler"
	install -d "$(APPDIR)"
	printf '%s\n' \
		'[Desktop Entry]' \
		'Type=Application' \
		'Name=AI Scheduler' \
		'Comment=Run scheduled local AI CLI routines' \
		'Exec=$(BINDIR)/ai-scheduler' \
		'Terminal=false' \
		'Categories=Utility;Development;' \
		> "$(APPDIR)/ai-scheduler.desktop"
	@echo "Installed $(BINDIR)/ai-scheduler"
	@echo "Config and run history were not modified."

update:
	git pull --ff-only
	$(MAKE) install

build:
	npm install
	npm run build
	cargo build --release --manifest-path src-tauri/Cargo.toml

test:
	cargo test --manifest-path src-tauri/Cargo.toml
	npm run build

dev:
	npm run tauri dev

bootstrap-config:
	install -d "$(CONFIG_DIR)"
	test -f "$(CONFIG_DIR)/config.toml" || cp config.example.toml "$(CONFIG_DIR)/config.toml"
	@echo "Config exists at $(CONFIG_DIR)/config.toml"

clean:
	rm -rf dist node_modules src-tauri/target
