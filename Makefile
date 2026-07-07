APP  := Firefox Router.app
DEST := $(HOME)/Applications
BIN  := target/release/ff-router
LSREGISTER := /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

.PHONY: build bundle install uninstall test

build:
	cargo build --release

# Assemble a self-contained .app bundle around the release binary.
bundle: build
	rm -rf "$(APP)"
	mkdir -p "$(APP)/Contents/MacOS"
	cp Info.plist "$(APP)/Contents/Info.plist"
	cp "$(BIN)" "$(APP)/Contents/MacOS/ff-router"
	printf 'APPL????' > "$(APP)/Contents/PkgInfo"
	codesign --force --sign - "$(APP)"

# Install to ~/Applications and register with Launch Services.
install: bundle
	mkdir -p "$(DEST)"
	rm -rf "$(DEST)/$(APP)"
	cp -R "$(APP)" "$(DEST)/$(APP)"
	rm -rf "$(APP)"
	"$(LSREGISTER)" -f "$(DEST)/$(APP)"
	@echo
	@echo "Installed to $(DEST)/$(APP)"
	@echo "Now set 'Firefox Router' as your default browser:"
	@echo "  System Settings > Desktop & Dock > Default web browser"

uninstall:
	rm -rf "$(DEST)/$(APP)"
	"$(LSREGISTER)" -u "$(DEST)/$(APP)" 2>/dev/null || true

test:
	cargo test
