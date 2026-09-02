CARGO ?= cargo
ARCH ?= x86_64

ifeq ($(ARCH),x86_64)
TARGET := x86_64-unknown-uefi
BOOT_NAME := BOOTX64.EFI
QEMU ?= qemu-system-x86_64
QEMU_ACCEL ?= kvm
else ifeq ($(ARCH),aarch64)
TARGET := aarch64-unknown-uefi
BOOT_NAME := BOOTAA64.EFI
QEMU ?= qemu-system-aarch64
# The development host is typically x86_64, so hardware acceleration is not
# assumed for the aarch64 guest.
QEMU_ACCEL ?= tcg
else
$(error unsupported ARCH '$(ARCH)'; supported: x86_64, aarch64)
endif

TARGET_DIR := ../target/real-hw-test
PROFILE := release
BINARY := $(TARGET_DIR)/$(TARGET)/$(PROFILE)/uart-16550-real-hw-test.efi
ARTIFACT := build/$(BOOT_NAME)
BUILD := CARGO_TARGET_DIR=$(TARGET_DIR) $(CARGO) build --locked
CLIPPY := CARGO_TARGET_DIR=$(TARGET_DIR) $(CARGO) clippy --locked

.PHONY: all artifact check qemu qemu-tcg ci-qemu install clean

all: artifact

artifact: $(ARTIFACT)

$(ARTIFACT): FORCE
	$(BUILD) --target $(TARGET) --release
	mkdir -p $(dir $(ARTIFACT))
	cp $(BINARY) $(ARTIFACT)

check:
	$(CARGO) fmt --check
	$(CLIPPY) --target x86_64-unknown-uefi --release -- -D warnings
	$(CLIPPY) --target aarch64-unknown-uefi --release -- -D warnings
	bash -n scripts/*.sh
	$(MAKE) artifact

qemu: artifact
	ARCH="$(ARCH)" QEMU="$(QEMU)" QEMU_ACCEL="$(QEMU_ACCEL)" OVMF="$(OVMF)" \
		./scripts/run-qemu.sh $(QEMU_ARGS)

qemu-tcg:
	$(MAKE) qemu QEMU_ACCEL=tcg

ci-qemu: artifact
	ARCH="$(ARCH)" QEMU="$(QEMU)" OVMF="$(OVMF)" \
		./scripts/run-qemu-ci.sh $(ARTIFACT)

install: artifact
	USB_MOUNT="$(USB_MOUNT)" ./scripts/install-usb.sh $(ARTIFACT)

clean:
	$(CARGO) clean --target-dir $(TARGET_DIR)
	rm -rf build

.PHONY: FORCE
FORCE:
