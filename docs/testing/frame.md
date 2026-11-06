# Test scenarios: Frame

Scenarios for parsing and encoding Milter protocol frames. Conventions are in [README.md](README.md).

## Parsing complete and incomplete frames

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| FRM-001 | unit | A buffer holding one complete frame: length 6, command `H`, data `mail\0` | the frame is parsed | the frame has command `H` and data `mail\0`, and the buffer is empty | [api: Frame](../reference/api-specification.md#frame) |
| FRM-002 | unit | A buffer holding two complete frames back to back | the buffer is parsed twice | each parse returns one frame, in order, and the buffer is then empty | [api: Frame](../reference/api-specification.md#frame) |
| FRM-003 | unit | A buffer holding 3 bytes, less than a length field | the buffer is parsed | no frame is returned, no error is raised, and the buffer is unchanged | [api: Frame](../reference/api-specification.md#frame) |
| FRM-004 | unit | A buffer holding a full length field and only part of the data | the buffer is parsed | no frame is returned, no error is raised, and the buffer is unchanged | [api: Frame](../reference/api-specification.md#frame) |
| FRM-005 | unit | A complete frame delivered one byte at a time | the buffer is parsed after each byte | no frame is returned until the last byte arrives, then exactly one frame | [api: Frame](../reference/api-specification.md#frame) |
| FRM-006 | unit | A frame with length 1: a command and no data | the frame is parsed | the frame has empty data | [protocol: Message format](../reference/milter-protocol.md#message-format) |
| FRM-007 | unit | A buffer whose length field is 0 | the buffer is parsed | a protocol error is returned | [protocol: Message format](../reference/milter-protocol.md#message-format) |

## Size limits

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| FRM-020 | unit | A maximum of 131,072 bytes (the default `per_connection_buffer`) and a buffer holding only a length field announcing 200,000 bytes of data | the buffer is checked | `FrameTooLarge` is returned at once, before the data arrives and without allocating for it | [api: Frame](../reference/api-specification.md#frame) |
| FRM-021 | unit | A maximum of 65,540 bytes and a frame with 65,535 data bytes (65,540-byte packet) | the frame is parsed | the frame is returned | [configuration: ResourceLimitsConfig](../reference/configuration.md#resourcelimitsconfig) |
| FRM-022 | unit | A maximum of 65,540 bytes and a frame with 65,536 data bytes | the buffer is checked | `FrameTooLarge` is returned | [configuration: ResourceLimitsConfig](../reference/configuration.md#resourcelimitsconfig) |
| FRM-023 | unit | The default maximum and a body chunk of 65,535 bytes, the largest either MTA sends | the frame is parsed | the frame is returned | [protocol: Headers and body](../reference/milter-protocol.md#headers-and-body) |
| FRM-024 | unit | The default maximum and a header frame of 100,000 data bytes | the frame is parsed | the frame is returned | [protocol: Message format](../reference/milter-protocol.md#message-format) |

## Encoding

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| FRM-040 | unit | A frame with command `c` and empty data | the frame is written | the output is the bytes 0, 0, 0, 1 and `c` | [protocol: Message format](../reference/milter-protocol.md#message-format) |
| FRM-041 | unit | A frame with command `h` and 10 data bytes | the frame is written | the length field is 11 in network byte order, followed by `h` and the data | [protocol: Message format](../reference/milter-protocol.md#message-format) |
| FRM-042 | property | Any command byte and any data of 0 to 65,535 bytes | a frame is written and the output parsed | the parsed frame equals the original | [api: Frame](../reference/api-specification.md#frame) |

## Robustness

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| FRM-060 | property | Arbitrary input bytes, mixing random bytes with plausible length fields, and a maximum between 5 and 1,024 bytes | the input is parsed repeatedly until no frame remains | parsing never panics, every frame returned fits within the maximum, and an incomplete frame left in the buffer is shorter than the maximum | [api: Frame](../reference/api-specification.md#frame) |
| FRM-061 | property | Any sequence of valid frames, split at random points into chunks | the chunks are fed to the parser one by one | the frames come out complete and in order | [api: Frame](../reference/api-specification.md#frame) |
