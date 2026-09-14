# Native Datatype API Reference

All APIs below use receiver syntax. Methods return a new value unless marked
mutating by the language's collection write-back rules.

## Type annotations

```adesh
let name: string = "Ada";
let count: i32 = 5;
let values: [i32] = [1, 2, 3];
let point: (f64, f64) = (10.0, 20.0);
let unique: set = {1, 2, 3};
let config: object = {port: 8080};
let value: complex = complex(3, 4);
```

`number`/`f64` are floating-point numeric values, while `i8` through `i128`
and `u8` through `u128` select fixed-width integer representations. `string`,
`bool`, `char`, `tuple`, `set`, `object`, and `complex` are native type names.
Array annotations use the existing `[Element]` and `[Element; Capacity]`
forms.

Complex values also support imaginary literals:

```adesh
let imaginary: complex = 5j;
let combined: complex = 5j + 2;
```

## Strings

`length()`, `isEmpty()`, `trim()`, `trimStart()`, `trimEnd()`,
`toLowerCase()`, `toUpperCase()`, `split(delimiter)`, `lines()`, `chars()`,
`substring(start, end?)`, `slice(start, end?)`, `charAt(index)`,
`indexOf(value)`, `lastIndexOf(value)`, `includes(value)`,
`startsWith(value)`, `endsWith(value)`, `replace(search, replacement)`,
`repeat(count)`, `reverse()`, `capitalize()`, `padStart(width, fill?)`,
`padEnd(width, fill?)`, `isAscii()`, `isNumeric()`, `isAlphabetic()`,
`isAlphanumeric()`.

## Arrays

`length()`, `isEmpty()`, `first()`, `last()`, `push(value)`, `pop()`,
`shift()`, `unshift(value)`, `insert(index, value)`, `remove(index)`,
`clear()`, `extend(values)`, `concat(values...)`, `slice(start, end?)`,
`join(separator)`, `contains(value)`, `indexOf(value)`,
`lastIndexOf(value)`, `count(value)`, `sort()`, `reverse()`, `distinct()`,
`sum()`, `min()`, `max()`, `toSet()`, `toTuple()`, `map(fn)`, `filter(fn)`,
`reduce(fn, initial?)`, `find(fn)`, `findIndex(fn)`, `some(fn)`,
`every(fn)`, `forEach(fn)`, `flat()`.

## Tuples

`length()`, `isEmpty()`, `first()`, `last()`, `get(index)`,
`contains(value)`, `includes(value)`, `indexOf(value)`,
`slice(start, end?)`, `toArray()`.

## Sets

`length()`, `isEmpty()`, `add(value)`, `remove(value)`, `delete(value)`,
`clear()`, `has(value)`, `contains(value)`, `union(other)`,
`intersection(other)`, `difference(other)`, `symmetricDifference(other)`,
`isSubsetOf(other)`, `isSupersetOf(other)`, `toArray()`.

## Dictionaries and objects

Object literals are string-keyed dictionaries at runtime:
`length()`, `isEmpty()`, `has(key)`, `containsKey(key)`, `get(key)`,
`getOr(key, default)`, `keys()`, `values()`, `entries()`, `merge(other)`,
`toArray()`.

## Numbers

`abs()`, `floor()`, `ceil()`, `round()`, `trunc()`, `fract()`, `sqrt()`,
`cbrt()`, `sign()`, `pow(exponent)`, `clamp(min, max)`, `isFinite()`,
`isInfinite()`, `isNaN()`, `isInteger()`, `isEven()`, `isOdd()`,
`toString()`, `toFixed(digits)`.

## Complex numbers

`real()`, `imag()`, `magnitude()`, `abs()`, `phase()`, `argument()`,
`conjugate()`, `pow(exponent)`, `toString()`.
