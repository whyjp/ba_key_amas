# Third-party notices

## Microsoft Hybrid Computer Use

The in-process desktop control in `rust/crates/hcu/src/native.rs` re-implements, in Rust against Win32, the
behaviour of the Windows provider of Hybrid Computer Use (screenshot delivery size, zoom geometry, the
frame-point to native-pixel mapping, absolute pointer coordinates, the layout check of a frame, the settle
loop). No source file is copied; the C# files were read as the specification.

- Project: https://github.com/microsoft/Hybrid-Computer-Use
- Reference revision: `8ce82b310b014ab1133b28febc2eca81527e43e6` (2026-11-18)
- License: MIT

```
MIT License

Copyright (c) 2026 Hybrid Computer Use authors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
