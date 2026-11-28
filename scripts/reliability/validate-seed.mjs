const seed = process.argv[2] ?? '';
const digits = seed.startsWith('+') ? seed.slice(1) : seed;
if (!digits || /[^0-9]/.test(digits) || BigInt(digits) > 18446744073709551615n) {
  console.error('SEED must be an unsigned 64-bit integer');
  process.exitCode = 1;
}
