.PHONY: default
default: build

.PHONY: build
build:
	$(MAKE) -C x86 build
	$(MAKE) -C riscv build

.PHONY: build_x86
build_x86:
	$(MAKE) -C x86 build

.PHONY: build_riscv
build_riscv:
	$(MAKE) -C riscv build

.PHONY: run
run:
	$(MAKE) -C x86 run
	$(MAKE) -C riscv run

.PHONY: run_x86
run_x86:
	$(MAKE) -C x86 run

.PHONY: run_riscv
run_riscv:
	$(MAKE) -C riscv run

.PHONY: clean
clean:
	$(MAKE) -C x86 clean
	$(MAKE) -C riscv clean
