# fatcat 🐈

> Hunt down the fat files hogging your disk space.

A blazing-fast CLI tool to find large files using parallel directory traversal.

## Install

```bash
cargo install --path .
```

## Usage

```bash
fatcat [PATH] [OPTIONS]
```

| Option                | Description                      |
|-----------------------|----------------------------------|
| `-s, --size <MB>`     | Minimum file size (default: 100) |
| `-t, --top <N>`       | Show top N files (default: 20)   |
| `-o, --output <FILE>` | Save results to log file         |
| `-v, --verbose`       | Show detailed statistics         |
| `-h, --help`          | Show help                        |

## Examples

```bash
fatcat                        # Scan current directory
fatcat /home -s 500           # Find files >= 500MB
fatcat ~/Downloads -t 10      # Show top 10 largest files
fatcat -v -o report.log       # Verbose mode + save log
```

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE).