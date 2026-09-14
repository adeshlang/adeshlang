# Makefile for AdeshLang AOT Runtime Library
#
# This builds the runtime support library for AOT-compiled AdeshLang code.
# The library provides memory tracking and RAII support.

CC = gcc
AR = ar
CFLAGS = -O2 -Wall -Wextra -fPIC
DEBUG_CFLAGS = -g -DVYOM_DEBUG_MEMORY

# Output files
LIB_RELEASE = libadesh_aot_runtime.a
LIB_DEBUG = libadesh_aot_runtime_debug.a
OBJ_RELEASE = adesh_aot_runtime.o
OBJ_DEBUG = adesh_aot_runtime_debug.o

# Source files
SRC = adesh_aot_runtime.c

.PHONY: all clean debug release

all: release debug

release: $(LIB_RELEASE)

debug: $(LIB_DEBUG)

$(LIB_RELEASE): $(OBJ_RELEASE)
	$(AR) rcs $@ $^
	@echo "Built release library: $@"

$(LIB_DEBUG): $(OBJ_DEBUG)
	$(AR) rcs $@ $^
	@echo "Built debug library: $@"

$(OBJ_RELEASE): $(SRC)
	$(CC) $(CFLAGS) -c $< -o $@

$(OBJ_DEBUG): $(SRC)
	$(CC) $(CFLAGS) $(DEBUG_CFLAGS) -c $< -o $@

clean:
	rm -f $(OBJ_RELEASE) $(OBJ_DEBUG) $(LIB_RELEASE) $(LIB_DEBUG)
	@echo "Cleaned build artifacts"

# Test compilation
test: $(LIB_DEBUG)
	@echo "Testing library..."
	@echo "  Size: $$(stat -f%z $(LIB_DEBUG) 2>/dev/null || stat -c%s $(LIB_DEBUG) 2>/dev/null) bytes"
	@echo "  Symbols:"
	@nm $(LIB_DEBUG) | grep adesh_rt || true

# Install (optional)
install: $(LIB_RELEASE) $(LIB_DEBUG)
	@echo "Installing libraries to ../target/release/"
	@mkdir -p ../target/release/
	@cp $(LIB_RELEASE) ../target/release/
	@cp $(LIB_DEBUG) ../target/release/
	@echo "Installed."
