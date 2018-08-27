# places-tool

Tool to convert places databases to other things. Currently can convert to:

- anonymized places databases
- mentat databases

## Background

This was pulled out of https://github.com/mozilla/application-services/pull/191 (where it had trouble due to requiring sqlcipher), and before that it was two separate tools, https://github.com/thomcc/mentat-places-test and https://github.com/thomcc/anonymize-places.

## Usage

### `to-mentat` Usage

```
$ cargo run --release -- to-mentat -fr
```

This will use the largest places.sqlite in your profiles (if it can find it -- see below on how to point it explicitly at a sqlite file), and will output `mentat_places.db`. There are more arguments (documented above) if you want/need to specify the output or input explicitly.

The `-f` overwrites `mentat_places.db` if it already exists, and `-r` is a more realistic workload, avoiding doing everything in a single-transaction (and doing one transaction per place instead), which has an impact on final database size.

You can insert things more quickly by omitting the `-r` flag. Actually, with the new schema the difference in size is only around 10% (with the old schema it was around 30%). I don't really understand where the difference in size between these comes from, so this is fairly surprising.

#### `to-mentat` full documentation

```
places-tool-to-mentat
Convert a places database to a mentat database

USAGE:
    places-tool to-mentat [FLAGS] [ARGS]

FLAGS:
    -f, --force        Overwrite OUTPUT if it already exists
    -h, --help         Prints help information
    -r, --realistic    Insert everything with one transaction per visit. This is a lot slower, but is a more realistic
                       workload. It produces databases that are ~30% larger (for me).
    -v                 Sets the level of verbosity (pass up to 3 times for more verbosity -- e.g. -vvv enables trace
                       logs)
    -V, --version      Prints version information

ARGS:
    <OUTPUT>    Path where we should output the mentat db (defaults to ./mentat_places.db)
    <PLACES>    Path to places.sqlite. If not provided, we'll use the largest places.sqlite in your firefox profiles
```

### `anonymize` Usage

```
$ cargo run --release -- anonymize -f
$ cargo run --release -- to-mentat -fr mentat_places_anon.db places_anonymized.sqlite
```

The first command produces `places_anonymized.sqlite` (again, using the largest places.sqlite in your firefox profiles as input by default), which is your places.sqlite but with all strings replaced with random alphanumeric ascii strings of the same length. The second command produces `mentat_places_anon.db` from it.

#### `anonymize` full documentation

```
places-tool-anonymize
Anonymize a places database

USAGE:
    places-tool anonymize [FLAGS] [ARGS]

FLAGS:
    -f, --force      Overwrite OUTPUT if it already exists
    -h, --help       Prints help information
    -v               Sets the level of verbosity (pass up to 3 times for more verbosity -- e.g. -vvv enables trace logs)
    -V, --version    Prints version information

ARGS:
    <OUTPUT>    Path where we should output the anonymized db (defaults to places_anonymized.sqlite)
    <PLACES>    Path to places.sqlite. If not provided, we'll use the largest places.sqlite in your firefox profiles
```
