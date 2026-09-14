# Datatype APIs

AdeshLang exposes native datatype behavior through receiver methods:
`value.method(arguments)`. The examples in this directory are executable
interpreter examples and avoid free-function collection APIs.

| Directory | Type |
|---|---|
| `strings` | UTF-8 strings |
| `arrays` | Dynamic and fixed arrays |
| `tuples` | Immutable positional values |
| `sets` | Unique collections and set algebra |
| `dictionaries` | String-keyed object maps |
| `numbers` | Numeric and integer helpers |
| `complex` | Complex numbers |

See [API_REFERENCE.md](API_REFERENCE.md) for signatures and return values.

`typed_datatypes.adesh` demonstrates annotations for every core datatype and
the `5j` imaginary-number literal. Each type directory also uses annotations
in its method example.
