PREFIX ?= /usr
BINDIR ?= $(PREFIX)/bin
TMPFILESDIR ?= $(PREFIX)/lib/tmpfiles.d

.PHONY: all build install uninstall clean

all: build

build:
	cargo build --release --locked

install:
	install -Dm755 target/release/mflm $(DESTDIR)$(BINDIR)/mflm
	install -Dm644 systemd-tmpfiles.conf $(DESTDIR)$(TMPFILESDIR)/mflm.conf

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/mflm $(DESTDIR)$(TMPFILESDIR)/mflm.conf

clean:
	cargo clean
