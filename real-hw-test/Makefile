CARGO ?= cargo
ARCH ?= x86_64
ARCHS := x86_64 aarch64

ifeq ($(ARCH),x86_64)
TARGET := x86_64-unknown-uefi
BOOT_NAME := BOOTX64.EFI
else ifeq ($(ARCH),aarch64)
TARGET := aarch64-unknown-uefi
BOOT_NAME := BOOTAA64.EFI
else
$(error unsupported ARCH '$(ARCH)'; supported: x86_64, aarch64)
endif

TARGET_DIR := ../target/real-hw-test
PROFILE := release
BINARY := $(TARGET_DIR)/$(TARGET)/$(PROFILE)/uart-16550-real-hw-test.efi
ARTIFACT := build/$(BOOT_NAME)

.PHONY: all artifact artifacts check clean

all: artifact

artifact: $(ARTIFACT)

$(ARTIFACT): FORCE
	CARGO_TARGET_DIR=$(TARGET_DIR) $(CARGO) build --locked --target $(TARGET) --release
	mkdir -p $(dir $(ARTIFACT))
	cp $(BINARY) $(ARTIFACT)

artifacts:
	for arch in $(ARCHS); do $(MAKE) artifact ARCH=$$arch || exit 1; done

check:
	$(CARGO) fmt --check
	CARGO_TARGET_DIR=$(TARGET_DIR) $(CARGO) clippy --locked --target $(TARGET) --release -- -D warnings
	$(MAKE) artifact

clean:
	$(CARGO) clean --target-dir $(TARGET_DIR)
	rm -rf build

.PHONY: FORCE
FORCE:

