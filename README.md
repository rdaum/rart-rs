# Ryan's Adaptive Radix Tree

This is yet another performant implementation of an Adaptive Radix Tree (ART) in Rust. ARTs are an ordered associative (key-value) structure that outperform BTrees for many use cases.

The implementation is based on the paper [The Adaptive Radix Tree: ARTful Indexing for Main-Memory Databases](https://db.in.tum.de/~leis/papers/ART.pdf) by Viktor Leis, Alfons Kemper, and Thomas Neumann.

I have not (yet) implemented the concurrent form described in later papers.

During implementation I looked at the following implementations, and borrowed some flow and ideas from them:
   * https://github.com/armon/libart/ A C implementation
   * https://github.com/rafaelkallis/adaptive-radix-tree C++ implementation
   * https://github.com/Lagrang/art-rs Another implementation in Rust. (The criterion benches I have were on the hole borrowed from here originally)

Performance boost can be gained through fixed sized keys and partials. There are a selection of key types and partials provided under the `partials` module.

I believe my implementation to be competitive, performance wise. The included benches compare against `BTreeMap`,
`HashMap`, and `art-rs` and show what you'd expect: performance between a hashtable and a btree for random inserts and 
retrievals, but sequential inserts and retrievals are much faster than either.   

Some notes:

  * Minimal external dependencies.
  * Compiles for `stable` Rust.
  * There are some bits of compartmentalized `unsafe` code for performance reasons, but the public API is safe.
  * Uses explicit SIMD optimizations for x86 SSE for the 16-child inner keyed node; an implementation for ARM NEON is also there, but doesn't really provide
much performance benefit.

More documentation to come. Still working at smoothing the rough corners. Contributions welcome.
