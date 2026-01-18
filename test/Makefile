.PHONY: default
default: build


.PHONY: build
build:
	cargo build --release \
		--target ./x86-unknown-none.json \
		-Z build-std=core,alloc,compiler_builtins \
		-Z build-std-features=compiler-builtins-mem \
		-Zjson-target-spec


.PHONY: run
run: | build
	@# QEMU/TCG allows better debuggability over QEMU/KVM
	qemu-system-i386 \
		-debugcon stdio \
		-device isa-debug-exit,iobase=0xf4,iosize=0x04 \
		-display none \
		-kernel target/x86-unknown-none/release/kernel \
		-m 64M \
		-machine q35,accel=tcg \
		-monitor none \
		-no-reboot \
		-nodefaults \
		-serial file:serial.pio.txt; \
	rc=$$? ; \
	if [ $$rc -ne 73 ]; then \
		echo "Test Run Failed" >&2 ; \
		exit 1 ; \
	fi
	grep "hello from serial via x86 port I/O" ./serial.pio.txt


.PHONY: clean
clean:
	cargo clean
