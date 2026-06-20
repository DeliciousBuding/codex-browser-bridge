# Dependency Graph

```mermaid
graph TD
    subgraph "Phase 1 — Core CDP & Page Assets"
        T1["T1: codex_execute_cdp<br/>(Lane A)"] --> T4["T4: Phase 1 Tests"]
        T3["T3: Browser Helpers<br/>(Lane A)"] --> T1
        T3 --> T2["T2: codex_page_assets<br/>(Lane B)"]
        T2 --> T4
    end

    subgraph "Phase 2 — Network Domain"
        T7["T7: Network Helpers<br/>(Lane A)"] --> T5["T5: network_cookies<br/>(Lane A)"]
        T7 --> T6["T6: network_set_cookie<br/>(Lane B)"]
        T5 --> T8["T8: Phase 2 Tests"]
        T6 --> T8
    end

    subgraph "Phase 3 — Integration & Review"
        T4 --> T9["T9: Build & Smoke Test"]
        T8 --> T9
        T9 --> T10["T10: Multi-agent Review<br/>(parallel lanes)"]
        T10 --> T11["T11: Governance Update"]
    end
```

## Parallel Lanes

### Phase 1
- **Lane A**: T3 → T1 (browser helpers, then CDP tool)
- **Lane B**: T2 (page assets, depends on T1 via T3)
- **Barrier**: After T1+T2+T3 complete → T4 (tests)

Actually, since this is a focused implementation in 2 files (`mcp.rs` + `browser.rs`), it's more efficient to implement serially:
1. All browser helpers (T3 + T7)
2. All MCP tools (T1 + T2 + T5 + T6)
3. All tests (T4 + T8)
4. Build + smoke (T9)
5. Review (T10)
6. Governance (T11)

No file-level merge conflicts because it's all additive.
