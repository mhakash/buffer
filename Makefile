.PHONY: run test bundle install icon

run:
	cargo run

test:
	cargo test

bundle:
	./scripts/bundle.sh

# Build + install/update /Applications/Workdesk.app and relaunch it.
install:
	./scripts/install.sh

icon:
	./scripts/make_icon.sh
