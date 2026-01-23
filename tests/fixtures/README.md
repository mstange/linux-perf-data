# Test Fixtures

### File Mode (standard perf.data format)

- `sleep.data` - Uncompressed perf data file (104-byte header)
- `sleep_compressed.data` - Zstd-compressed perf data file (104-byte header)

### Pipe Mode (streaming perf.data format)

- `sleep_compressed.pipe.data` - Zstd-compressed pipe mode data (16-byte header)
- `fibo_compressed.pipe.data` - Zstd-compressed pipe mode data with records spanning compressed chunk boundaries

## Generation

These files were generated using the following commands:

```bash
# File mode - Uncompressed
perf record -o sleep.data -k monotonic sleep 1

# File mode - Compressed with zstd
perf record -z -o sleep_compressed.data -k monotonic sleep 1

# Pipe mode - Compressed with zstd
perf record -z -o - sleep 1 > sleep_compressed.pipe.data
```

All files capture the sleep 1 second workload to enable comparison testing.
Except for `fibo_compressed.pipe.data`, which was generated using a custom workload to create records that span compressed chunk boundaries.
