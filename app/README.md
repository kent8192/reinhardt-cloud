# nuages

A Reinhardt project.

## Getting Started

### Using cargo-make (Recommended)

Install cargo-make:
```bash
cargo install cargo-make
```

Run the development server:
```bash
cargo make runserver
```

### Using manage command

```bash
# Run the development server
cargo run --bin manage runserver

# Run migrations
cargo run --bin manage migrate
```

### Using reinhardt-admin

Install [reinhardt-web](https://github.com/kent8192/reinhardt-web) CLI tools:
```bash
cargo install reinhardt-admin
```

```bash
# Create a new app
reinhardt-admin startapp myapp
```

## Common Tasks

### Development

```bash
cargo make dev              # Run checks + build + start server
cargo make dev-watch        # Development with auto-reload (requires bacon)
cargo make runserver-watch  # Start server with auto-reload (requires bacon)
```

### Database

```bash
cargo make makemigrations   # Create new migrations
cargo make migrate          # Apply migrations
```

### Testing

```bash
cargo make test             # Run all tests (uses cargo-nextest)
cargo make test-unit        # Run unit tests only
cargo make test-integration # Run integration tests only
cargo make test-watch       # Run tests with auto-reload (requires bacon)
```

### Project Management

```bash
cargo make check            # Check project for common issues
cargo make showurls         # Display all registered URL patterns
cargo make shell            # Run an interactive Rust shell (REPL)
cargo make collectstatic    # Collect static files into STATIC_ROOT
```

### Code Quality

```bash
cargo make fmt-check        # Check code formatting
cargo make fmt-fix          # Fix code formatting
cargo make clippy-check     # Check linting rules
cargo make clippy-fix       # Fix linting issues
cargo make quality          # Run all checks (format + lint)
cargo make quality-fix      # Fix all issues automatically
```

### Build

```bash
cargo make build            # Build in debug mode
cargo make build-release    # Build in release mode
cargo make ci               # Run CI pipeline (format, lint, build, test)
```

### Help

```bash
cargo make help             # Show all available tasks
```

## Generated with

This project was created using `reinhardt-admin startproject`.
