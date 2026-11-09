// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // G45+：`--headless` 是唯一的模式开关。不带它的调用**行为一个字都不变**
    // （GUI 仍是默认），评测靠它不开窗口跑一轮并把事件流打成 NDJSON。
    // 判定住在 `headless::wants_headless`（可单测），这里只做分派。
    let args: Vec<String> = std::env::args().skip(1).collect();
    if lfttdo_lib::headless::wants_headless(&args) {
        std::process::exit(run_headless(&args));
    }
    lfttdo_lib::run()
}

/// 无头模式的全部收尾都在这里：用法错误 → 2、超时 → 3、运行错误 → 1、正常 → 0。
/// 退出码是评测能读的唯一信号，所以每一支都明确，不容 `unwrap` 逃逸。
fn run_headless(args: &[String]) -> i32 {
    use lfttdo_lib::headless;
    let parsed = match headless::parse_args(args) {
        Ok(a) => a,
        // 帮助与用法错误同一条出口（都是「这次不跑」）：打印到 stderr，退出 2。
        Err(text) => {
            eprintln!("{text}");
            return 2;
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("创建运行时失败：{e}");
            return 1;
        }
    };
    match runtime.block_on(headless::run(parsed)) {
        Ok(()) => 0,
        Err(reason) if reason == "timeout" => 3,
        Err(reason) => {
            eprintln!("{reason}");
            1
        }
    }
}
