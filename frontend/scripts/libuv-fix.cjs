/**
 * Workaround for libuv assertion failure on Windows:
 *   "Assertion failed: result_size == sppi_size, file src\win\util.c, line 571"
 *
 * The native os.cpus() calls uv_cpu_info() which crashes on certain
 * Windows builds due to GetSystemProcessorPerformanceInformation returning
 * an unexpected buffer size (e.g. hybrid CPU architectures, parked cores).
 *
 * This preload script replaces os.cpus() with a safe stub that never
 * invokes the broken native call. Vite / chokidar only use os.cpus()
 * for parallelism hints, so a stub is perfectly fine.
 */
'use strict';

const os = require('os');

const cpuCount = os.availableParallelism ? os.availableParallelism() : 4;

const stubCpu = {
  model: 'stubbed (libuv-fix)',
  speed: 2400,
  times: { user: 0, nice: 0, sys: 0, idle: 0, irq: 0 },
};

os.cpus = () => Array.from({ length: cpuCount }, () => ({ ...stubCpu }));
