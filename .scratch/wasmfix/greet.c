__attribute__((import_module("wasi_snapshot_preview1"), import_name("fd_write")))
extern int fd_write(int fd, const void *iovs, int iovs_len, int *nwritten);

static const char message[] = "hello from adeshlink\n";
static unsigned written;

__attribute__((export_name("greet"))) int greet(void) {
  struct iovec { const char *base; unsigned len; } iov = { message, sizeof(message) - 1 };
  return fd_write(1, &iov, 1, &written);
}
