# 代码质量审查 Bug 修复进度(2026-08-26)

来源:2026-08-26 全库 thermo-nuclear 质量审查。本页只跟踪**已验证的真实 bug**(审查报告第一部分,共 6 项);报告第二部分的结构性重构建议不属于 bug,记录在文末"后续重构候选",不在本次修复范围。

每项修复要求:配回归测试;`cargo clippy` / `cargo fmt` / `cargo nextest run --workspace` 通过(Rust 项),前端项跑 web lint/typecheck/test,Android 项跑 `:app:testDebugUnitTest`。

| #   | Bug                                                                                                                 | 状态                                                                                                                                          | 测试                                                                                                                     |
| --- | ------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| 1   | `ft_job_get_result` Condvar 谓词反转,从不等待即返回 `FT_ETIMEOUT`(`pandar-network-plugin/src/file_transfer/job.rs`) | ✅ 已修:谓词改为 `\|state\| !state.finished`,删除冗余外层判断                                                                                 | `get_result_waits_for_completion_and_reports_the_job_ec`、`get_result_times_out_when_the_job_never_finishes`(含耗时断言) |
| 2   | Android WS `Outcome.Connected` 不可达,健康重连退避永不重置(`PrinterEventsRepository.kt`)                            | ✅ 已修:`Outcome` 改为按是否收到帧区分的 `ClosedAfterTraffic`/`ClosedWithoutTraffic`;健康会话重置退避;退避状态提取为可测的 `ReconnectBackoff` | `ReconnectBackoffTest`(3 项);`:app:testDebugUnitTest` 全绿                                                               |
| 3   | 前端 `?status=` 重定向反馈断线:`actionToast` 死状态无写入者(`dashboard-shell-store.ts`)                             | ✅ 已修:shell 布局直接从 URL searchParams 渲染 `ActionStatusToast`;整个死 store 删除                                                          | `dashboard-shell.test.tsx` 新增 "?status= 反馈为 toast" 回归;vitest 23 项全绿                                            |
| 4   | Jobs/Users/Agents 页授权 fail-open / 不可达条件                                                                     | ✅ 已修:策略提取到 `frontend/app/membership-policy.ts`(fail-closed),三个 server 页面统一调用                                                  | `membership-policy.test.ts`(8 项);tsc 通过                                                                               |
| 5   | Hub `redact_key_value` 只处理每个 key 的首次出现,同 key 第二个凭据泄漏进日志                                        | ✅ 已修:扫描并脱敏一行内所有出现位置;未构成键值对的出现跳过而非放弃整行                                                                       | `redacts_every_occurrence_of_repeated_keys_in_one_line`、`redacts_later_key_occurrence_when_earlier_one_is_not_a_pair`   |
| 6   | Agent MQTT 报告队列满时静默丢弃(含增量 materials patch),无指标/无重同步                                             | ✅ 已修:溢出时清空缓冲并向队列写入显式 overflow 错误,消费方(转发循环)按非 idle 失败退出,重试循环重新订阅并发送全新 `pushall` 完成重同步       | `report_queue_overflow_surfaces_an_error_instead_of_dropping_entries`;背压测试改为恰好填满队列                           |

## 后续重构候选(非 bug,另行安排)

见审查报告第二部分:pandar-protocol 统一 crate、shim 策略回迁 Rust、命令准入统一、cleanup/clear 原子性、printer_operations tagged enum、PG 测试 CI 化、前端 god-component 拆解等。
