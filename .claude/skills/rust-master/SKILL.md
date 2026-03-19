---
name: rust-master
description: Procedural guidance and best practices for advanced Rust programming, encompassing Async Rust, Tokio, Atomics, Concurrency, and idiomatic Rust Design Patterns. Use when architecting or implementing complex Rust applications.
---

# Rust Master: Async, Tokio, Atomics, and Patterns

This skill serves as the definitive guide for writing idiomatic, safe, and highly performant Rust code, drawing deep principles from "Rust Atomics and Locks" (Mara Bos), the official "Asynchronous Programming in Rust" book, and "Rust Design Patterns".

## 1. Asynchronous Programming in Rust (The Async Book)

### The `Future` Trait & Execution Model
- **Futures are Lazy**: A future in Rust (`std::future::Future`) does nothing until it is actively polled by an executor.
- **`Pin` and `Unpin`**: `async fn` creates state machines that often contain self-references. They must be `Pin`ned to ensure their memory location doesn't move. Use `Box::pin` or `tokio::pin!` when you need to poll dynamically.
- **The `Waker`**: When a future returns `Poll::Pending`, it must arrange for the `Waker` (from the `Context`) to be called when it can make progress. Never return `Pending` without setting up a wake-up mechanism.

### Concurrency Primitives (`join!`, `select!`, `spawn`)
- **`tokio::spawn`**: Spawns a new independent task onto the runtime. Use for background work. The spawned future must be `'static` and `Send`.
- **`tokio::join!`**: Awaits multiple futures concurrently on the *same* task. Completes when *all* futures complete.
- **`tokio::select!`**: Awaits multiple futures concurrently, but completes when the *first* future completes, canceling the others. 
  - **Cancellation Safety**: Ensure the futures used in `select!` are cancellation-safe (e.g., dropping them midway doesn't corrupt state or lose data). Reading from a channel is cancellation-safe; writing to a stream or mutating external state midway might not be.

### Pitfalls to Avoid
- **Blocking the Executor**: Never perform CPU-heavy computations or synchronous I/O (`std::thread::sleep`, `std::fs`, locking a heavy `std::sync::Mutex`) inside an `async fn`. It starves the executor thread. Use `tokio::task::spawn_blocking` instead.
- **Holding `!Send` Types Across `.await`**: Do not hold a standard `std::sync::MutexGuard` across an `.await` point. It makes the future `!Send`, preventing it from running on a multi-threaded executor. Use `tokio::sync::Mutex` only if you strictly need the lock held across await points; otherwise, drop the std mutex before awaiting.

## 2. Atomics and Locks (Mara Bos)

### Memory Ordering
Choosing the correct `std::sync::atomic::Ordering` is critical for correctness and performance.
- **`Relaxed`**: No synchronization. Only guarantees atomicity of the single operation. Use for simple counters (`fetch_add`) where the order of other memory accesses doesn't matter.
- **`Acquire` and `Release`**: 
  - **`Release` (Store)**: Ensures that all memory operations occurring *before* the store are visible to any thread that performs an `Acquire` load.
  - **`Acquire` (Load)**: Ensures that all memory operations occurring *after* the load see the effects of the corresponding `Release` store.
  - *Pattern*: Use `Release` when writing a flag that data is ready; use `Acquire` when reading that flag to access the newly ready data.
- **`SeqCst` (Sequentially Consistent)**: The strongest ordering. Imposes a total global order on all `SeqCst` operations. Use when multiple atomics need to be observed in exactly the same order by all threads.

### Building Concurrency Primitives
- **Spin Loops**: If building a custom spinlock, always use `std::hint::spin_loop()` inside the busy wait to prevent CPU hogging and optimize power usage.
- **Thread Parking**: For long waits, avoid spinning. Use `std::thread::park` and `std::thread::Thread::unpark` (or OS-level futexes) to put threads to sleep.
- **False Sharing**: When multiple threads heavily mutate independent atomics that reside on the same CPU cache line, performance degrades rapidly. Align structs to cache line boundaries (`#[repr(align(64))]`) for highly contended atomics.

## 3. Idiomatic Rust Design Patterns

### Structural & Creational Patterns
- **Builder Pattern**: Construct complex objects with many optional parameters by chaining methods (e.g., `ServerBuilder::new().port(8080).build()`).
- **Newtype Pattern**: Wrap a primitive type in a single-element tuple struct (`struct UserId(u64);`). Enforces type safety at compile time, preventing accidental mix-ups of raw types.
- **Type State Pattern**: Encode the state machine of an object into the type system to make invalid states unrepresentable.
  - Example: `struct Connection<State>;`. Methods consuming `Connection<Unauthenticated>` return `Connection<Authenticated>`, enforcing proper workflow via the compiler.

### Behavioral Patterns
- **RAII (Resource Acquisition Is Initialization)**: Use the `Drop` trait to ensure resources (locks, file handles, memory, network connections) are deterministically released when they go out of scope.
- **Strategy via Traits**: Use `Box<dyn Trait>` for dynamic dispatch when the exact type isn't known until runtime, or `impl Trait` / Generics for static dispatch when performance and monomorphization are paramount.

### Anti-Patterns to Avoid
- **`Clone` to satisfy the Borrow Checker**: Re-evaluate ownership and lifetimes before blindly cloning data to make compilation errors go away.
- **`Deref` Polymorphism**: Do not implement `Deref` to simulate OOP inheritance. `Deref` is strictly for smart pointers.
- **Panic on Expected Errors**: Never use `unwrap()` or `expect()` in production code unless you are mathematically certain it cannot fail (and leave a comment explaining why). Pervasively use `?` and bubble up `Result`s with custom `thiserror` enums.

## 4. Rust Naming Conventions & Bounded Contexts

### Naming Conventions
Follow standard Rust naming conventions strictly, enforcing the Ubiquitous Language of your domain:
- **Types/Traits**: `UpperCamelCase` (e.g., `MessageFilter`, `ConnectionState`).
- **Variables/Functions/Macros/Modules**: `snake_case` (e.g., `process_header`, `client_ip`).
- **Constants/Statics**: `SCREAMING_SNAKE_CASE` (e.g., `MAX_BUFFER_SIZE`).
- **Avoid Stuttering**: Do not repeat the module name in the type name. Use `filter::Config`, not `filter::FilterConfig`.

### Enforcing Bounded Contexts
Use Rust's module system to enforce DDD Bounded Contexts:
- **Privacy by Default**: Keep types and fields private (`pub(crate)` or `pub(super)`) by default. Only expose what is explicitly required by the public API of the Bounded Context.
- **Module Isolation**: Use modules (`mod`) to group related aggregates and services. An external module should ideally only interact with the Aggregate Root of another module.
- **Workspace Crates**: For strict Bounded Contexts in large applications, separate them into distinct Rust crates within a Cargo workspace.

## 5. Best Practices for This Workspace
1. **Tooling**: Always run `cargo clippy` and address all warnings. Format code with `cargo fmt`.
2. **Testing**: Write thorough unit tests using `#[tokio::test]` for async code. Consider `loom` for testing highly concurrent or lock-free structures.
3. **Documentation**: Document all public APIs with rustdoc (`///`), providing `# Examples` blocks that serve as doctests.

## Related Skills

- For applying TDD to Rust development, see **canon-tdd**.
- For logging and observability in the application, use **logging-principles**.
- For overarching architectural patterns like Clean Architecture, use **design-master**.
- For implementing resilient external API integrations, refer to **external-integration**.