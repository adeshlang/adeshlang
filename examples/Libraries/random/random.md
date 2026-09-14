# AdeshLang `Random` Standard Library

Comprehensive, production-grade randomness ecosystem for AdeshLang. Built with statistically sound PRNG engines, unbiased range sampling, stateful deterministic generators, sampling & Fisher-Yates shuffling, UUID generation, continuous/discrete probability distributions, and cryptographic entropy boundaries.

---

## 1. Importing `Random`

Unlike built-in global objects, `Random` is imported explicitly:

```adesh
import Random;
```

You can also import specific functions or use custom aliases:

```adesh
import Random as R;

from Random import int, float, choice, seed;
```

---

## 2. Basic Usage

```adesh
import Random;

let n = Random.int(1, 100);       // 1 <= n < 100
let f = Random.float();           // 0.0 <= f < 1.0
let b = Random.bool();            // true or false (50/50)
let byteVal = Random.byte();      // 0..255
let bytesArr = Random.bytes(16);   // Array of 16 random u8 bytes
```

---

## 3. Integer Randomness & Unbiased Range Semantics

All bounded integer generation uses rejection sampling (Lemire's algorithm) to guarantee **zero modulo bias**.

```adesh
Random.int(1, 100);             // 1 <= val < 100  (half-open range)
Random.intInclusive(1, 100);    // 1 <= val <= 100 (inclusive range)
Random.intExclusive(1, 100);    // 1 < val < 100   (exclusive range)

// Full representable width integer generators
Random.u8();
Random.u16();
Random.u32();
Random.u64();
Random.u128();

Random.i8();
Random.i16();
Random.i32();
Random.i64();
Random.i128();
```

---

## 4. Floating-Point Randomness

```adesh
Random.float();                 // 0.0 <= val < 1.0
Random.floatRange(10.0, 50.0);  // 10.0 <= val < 50.0
Random.floatInclusive(0.0, 1.0);// 0.0 <= val <= 1.0

Random.f32();                   // 32-bit single precision float
Random.f64();                   // 64-bit double precision float
```

---

## 5. Strings, Chars, and Identifiers

```adesh
Random.string(16);              // Alphanumeric string of length 16
Random.alphanumeric(32);        // Alphanumeric string [a-zA-Z0-9]
Random.ascii(20);               // Printable ASCII string (code points 32..126)
Random.hex(64);                 // Hexadecimal string [0-9a-f]
Random.stringFrom("ABCDEF012345", 10); // Custom character set sampling

Random.char();                  // Random valid Unicode scalar character
Random.charFrom("xyz!");        // Random character from string

Random.uuid();                  // RFC 4122 v4 UUID string (e.g. "f47ac10b-58cc-4372-a567-0e02b2c3d479")
Random.token(32);               // URL-safe token
Random.hexToken(16);            // Hex token
Random.base64Token(24);         // URL-safe Base64 token
```

---

## 6. Collection Shuffling & Sampling

```adesh
let colors = ["red", "green", "blue", "yellow", "purple"];

// Uniform random element selection
let color = Random.choice(colors);

// In-place Fisher-Yates shuffle
Random.shuffle(colors);

// Non-mutating shuffled copy
let newShuffled = Random.shuffled(colors);

// Sampling k elements without replacement
let sampleList = Random.sample(colors, 3);

// Sampling k elements with replacement
let repeatedSample = Random.sampleWithReplacement(colors, 10);

// Weighted choice
let loot = Random.weightedChoice([
    ["common", 90],
    ["rare", 9],
    ["legendary", 1]
]);

// Streaming reservoir sampling (k items from stream in O(k) memory)
let streamSample = Random.reservoirSample(colors, 2);
```

---

## 7. Seeded PRNG & Deterministic Reproducibility

For simulations, procedural generation, and reproducible unit tests, instantiate a seeded `Rng` object:

```adesh
import Random;

let rng = Random.seed(12345);

let a = rng.int(1, 100);
let b = rng.float();
let c = rng.bool();
let d = rng.string(10);
```

### Deterministic State & Methods
- `rng.int(min, max)` / `rng.intInclusive(min, max)`
- `rng.float()` / `rng.float(min, max)`
- `rng.bool()` / `rng.bool(p)`
- `rng.byte()` / `rng.bytes(count)`
- `rng.string(length)`
- `rng.choice(collection)`
- `rng.shuffle(collection)`
- `rng.sample(collection, k)`
- `rng.clone()`: Creates an independent copy with identical state
- `rng.fork()`: Forks a new deterministic child generator
- `rng.state()` / `rng.restore(stateArr)`: Serialises and restores state array `[u64; 4]`

---

## 8. Statistical Distributions

```adesh
// Convenience function sampling
let normSample = Random.normal(0.0, 1.0);        // Gaussian Normal(mean, stdDev)
let unifSample = Random.uniform(-10.0, 10.0);    // Uniform continuous U(min, max)
let expSample  = Random.exponential(1.5);        // Exponential Exp(lambda)
let binomSample = Random.binomial(20, 0.5);      // Binomial Binomial(n, p)
let poisSample = Random.poisson(4.0);            // Poisson Poisson(lambda)
let geomSample = Random.geometric(0.3);          // Geometric Geometric(p)
let gammaSample = Random.gamma(2.0, 2.0);        // Gamma Gamma(alpha, beta)
let logNormSample = Random.logNormal(0.0, 0.25); // LogNormal LogNormal(mean, stdDev)

// Distribution Objects (reusable sampling configuration)
let dist = Random.Normal(100.0, 15.0);
let iqSample1 = dist.sample();
let iqSample2 = dist.sample();
```

---

## 9. Secure Entropy Boundary

Standard `Random` functions use high-speed **Xoshiro256++** pseudo-randomness for simulation, game, and algorithmic performance.

For security-sensitive operations (cryptographic keys, passwords, session tokens), use explicit OS entropy:

```adesh
let secureKey = Random.secureBytes(32);
```

Never use standard PRNG for cryptographic keys.
