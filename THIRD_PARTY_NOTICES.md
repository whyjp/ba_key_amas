# 서드파티 고지

## Microsoft Hybrid Computer Use

`scanner/crates/hcu/src/native.rs`의 프로세스 내 데스크톱 제어는 Hybrid Computer Use의 Windows 제공자 동작을
Win32 기반 Rust로 다시 구현한 것이다. 다시 구현한 동작은 스크린샷 전달 크기, 줌 기하, 프레임 좌표를 원본 픽셀로
옮기는 매핑, 절대 포인터 좌표, 프레임 레이아웃 검사, 안정화 루프다. 소스 파일은 복사하지 않았고 C# 파일은
명세로만 읽었다. 읽기용으로 보관한 원본 사본은 `third_party/hcu-reference/`에 있고 빌드에 들어가지 않는다.

- 프로젝트: https://github.com/microsoft/Hybrid-Computer-Use
- 기준 리비전: `8ce82b310b014ab1133b28febc2eca81527e43e6` (2026-11-18)
- 라이선스: MIT

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
