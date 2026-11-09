#!/usr/bin/env node
// G45/G45+：会话级回归评测的**外部启动器**。
//
// 责任边界（照 ZCode usage/13 的取向）：LFTTDO 只当执行器 —— 案例集、环境隔离、
// 资源预算、评分全部在这里（仓库外的脚本），产品代码不为评测加任何开关
// （唯一入口是 `--headless`，它也是产品自己的命令行模式）。
//
// 用法：
//   node scripts/eval/run.mjs                 # 跑全部用例
//   node scripts/eval/run.mjs --case grep-rs  # 只跑一条
//   node scripts/eval/run.mjs --dry           # 只打印将要做的事（不启动应用）
//   node scripts/eval/run.mjs --boot-only     # 只验启动与隔离（G45 的旧范围）
//
// 两层判定：
//   1. **启动层**（`boot_only` 的用例）：进程起得来、保持运行、退出时进程树被回收。
//   2. **会话层**（带 `prompt` 的用例）：用 `lfttdo --headless` 跑真实一轮，
//      对 stdout 的 NDJSON 事件流断言 `expect.tools` / `expect.stop_reason` /
//      `expect.files`。**没有模型端点时这一层明确失败（exit 1）**，不静默跳过 ——
//      「评测全绿」如果其实一条会话断言都没跑，那是最坏的假信号。
//
// 这里跑的是**真实端点**（默认本机 Ollama），因此：
//   · 不进 CI（CI 无模型、无网络）；
//   · 每次运行的证据落 `scripts/eval/runs/<时间戳>/`（不提交，`.gitignore` 已覆盖）；
//   · 每个用例的判定是「期望的工具序列 + 产物断言」，不是「文本像不像」——
//     模型措辞天天变，而行为契约才是回归信号。

import { spawn } from "node:child_process";
import { cp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "..", "..");
const casesFile = join(here, "cases.json");

const argv = process.argv.slice(2);
const only = argv.includes("--case") ? argv[argv.indexOf("--case") + 1] : null;
const dry = argv.includes("--dry");
const bootOnly = argv.includes("--boot-only");

/** 跑一条命令并收集输出（超时杀进程树）。 */
function run(cmd, args, opts = {}) {
  return new Promise((res) => {
    const child = spawn(cmd, args, {
      cwd: opts.cwd ?? repo,
      shell: false,
      windowsHide: true,
      env: { ...process.env, ...(opts.env ?? {}) },
    });
    let out = "";
    let err = "";
    let timedOut = false;
    child.stdout?.on("data", (d) => (out += d));
    child.stderr?.on("data", (d) => (err += d));
    const timer = setTimeout(() => {
      timedOut = true;
      // 整棵进程树：Windows 用 taskkill /T，其余用负 pid（进程组）。
      if (process.platform === "win32") {
        spawn("taskkill", ["/PID", String(child.pid), "/T", "/F"], {
          windowsHide: true,
        });
      } else {
        try {
          process.kill(-child.pid, "SIGKILL");
        } catch {
          /* 已退出 */
        }
      }
    }, opts.timeoutMs ?? 180_000);
    child.on("close", (code) => {
      clearTimeout(timer);
      res({ code, out, err, timedOut });
    });
    child.on("error", (e) => {
      clearTimeout(timer);
      res({ code: -1, out, err: String(e), timedOut: false });
    });
  });
}

/** 构建指纹：评测结论必须能对应到**哪一份代码**。 */
async function fingerprint() {
  const sha = await run("git", ["rev-parse", "HEAD"]);
  const status = await run("git", ["status", "--porcelain"]);
  const pkg = JSON.parse(await readFile(join(repo, "package.json"), "utf8"));
  return {
    git_sha: sha.out.trim() || "(not a git checkout)",
    dirty: status.out.trim().length > 0,
    version: pkg.version,
    platform: `${process.platform}-${process.arch}`,
    node: process.version,
    at: new Date().toISOString(),
  };
}

/** 二进制路径（`--headless` 与 GUI 启动共用）。 */
function binaryPath({ release }) {
  const dir = release ? "release" : "debug";
  return process.platform === "win32"
    ? join(repo, "src-tauri", "target", dir, "lfttdo.exe")
    : join(repo, "src-tauri", "target", dir, "lfttdo");
}

/**
 * 隔离环境：独立数据目录 + 独立 HOME，评测不碰用户真实配置/会话/记忆。
 * `APPDATA` 也要指过去 —— Tauri 的数据目录解析走它。
 */
function isolatedEnv(profileDir) {
  return {
    HOME: profileDir,
    USERPROFILE: profileDir,
    APPDATA: join(profileDir, "roaming"),
    LOCALAPPDATA: join(profileDir, "local"),
  };
}

/**
 * 起 GUI 应用（启动层判定用）。
 *
 * 评测用的是**发布版二进制**（`npm run tauri build` 的产物）而不是 dev server：
 * dev 会拉一个 Vite 端口、还带热重载，评测要的是确定的东西。
 * 二进制不存在就明确失败，不偷偷降级成 dev。
 */
async function startApp(profileDir) {
  const exe = binaryPath({ release: true });
  if (!existsSync(exe)) {
    throw new Error(
      `找不到发布版二进制：${exe}\n先跑 \`npm run tauri build\`（评测不替你构建）。`,
    );
  }
  const child = spawn(exe, [], {
    cwd: profileDir,
    env: { ...process.env, ...isolatedEnv(profileDir) },
    windowsHide: false,
    detached: process.platform !== "win32",
  });
  return child;
}

/** 端点探测：一个明确的 GET，避免「跑了一轮才发现没端点」。 */
async function endpointAlive(baseUrl, timeoutMs = 4000) {
  try {
    const ac = new AbortController();
    const t = setTimeout(() => ac.abort(), timeoutMs);
    const res = await fetch(baseUrl.replace(/\/v1\/?$/, "/api/tags"), {
      signal: ac.signal,
    });
    clearTimeout(t);
    return res.ok;
  } catch {
    return false;
  }
}

/**
 * 用 `--headless` 跑真实一轮，返回结构化结果。
 *
 * 二进制优先级：发布版（评测对象）→ 调试版（本地迭代够快时用）。
 * 两个都没有就明确失败。结果里记 `binary` 字段，报告一眼能看出跑的是哪一个。
 */
async function runHeadless({ prompt, profileDir, session, baseUrl, model, timeoutSecs }) {
  let exe = binaryPath({ release: true });
  let binary = "release";
  if (!existsSync(exe)) {
    exe = binaryPath({ release: false });
    binary = "debug";
    if (!existsSync(exe)) {
      throw new Error(
        `找不到二进制（release 与 debug 都没有）：${exe}\n先跑 \`npm run tauri build\`，或用 \`cargo build\` 产一个调试版。`,
      );
    }
  }
  const args = ["--headless", "--prompt", prompt, "--data-dir", profileDir];
  if (session) args.push("--session", session);
  if (baseUrl) args.push("--base-url", baseUrl);
  if (model) args.push("--model", model);
  args.push("--timeout", String(timeoutSecs));

  const r = await run(exe, args, {
    env: isolatedEnv(profileDir),
    timeoutMs: (timeoutSecs + 15) * 1000,
  });
  const events = r.out
    .split(/\r?\n/)
    .filter((l) => l.trim().startsWith("{"))
    .map((l) => {
      try {
        return JSON.parse(l);
      } catch {
        return { t: "unparsable", raw: l };
      }
    });
  return { ...r, events, binary };
}

/** 从事件流里抽出可断言的形状。 */
function summarize(events) {
  return {
    tools: events.filter((e) => e.t === "tool_call").map((e) => e.call?.name ?? "?"),
    stop_reason: events.find((e) => e.t === "done")?.stop_reason ?? null,
    errors: events.filter((e) => e.t === "error").map((e) => e.message),
    text: events
      .filter((e) => e.t === "text_delta")
      .map((e) => e.text)
      .join(""),
    tool_results: events
      .filter((e) => e.t === "tool_result")
      .map((e) => ({ call_id: e.call_id, is_error: e.is_error, preview: e.preview })),
  };
}

/** 递归列文件（断言产物用；`files` 里写相对路径）。 */
async function listFiles(dir, base = dir) {
  let out = [];
  let entries;
  try {
    entries = await readdir(dir, { withFileTypes: true });
  } catch {
    return out;
  }
  for (const e of entries) {
    const full = join(dir, e.name);
    if (e.isDirectory()) {
      out = out.concat(await listFiles(full, base));
    } else {
      out.push(full.slice(base.length + 1).replaceAll("\\", "/"));
    }
  }
  return out;
}

/** 判定一条会话层用例。返回 `{ ok, failures, summary }`。 */
async function judge(c, result, profileDir) {
  const s = summarize(result.events);
  const failures = [];

  // 进程层：非零退出 = 这一轮没跑完（超时是 3）。
  if (result.code !== 0) {
    failures.push(
      result.code === 3
        ? "整轮超时（exit 3）"
        : `进程退出码 ${result.code}${result.err ? `：${result.err.trim().slice(0, 400)}` : ""}`,
    );
  }

  // 协议层：stdout 必须**只有** JSON 行（诊断走 stderr）。
  const junk = result.out
    .split(/\r?\n/)
    .filter((l) => l.trim() && !l.trim().startsWith("{"));
  if (junk.length > 0) {
    failures.push(`stdout 里有非 NDJSON 行：${junk.slice(0, 3).join(" / ")}`);
  }

  const expect = c.expect ?? {};
  if (expect.stop_reason !== undefined && s.stop_reason !== expect.stop_reason) {
    failures.push(`期望 stop_reason=${expect.stop_reason}，实际 ${s.stop_reason}`);
  }
  if (Array.isArray(expect.tools)) {
    // 序列断言：完全相同（顺序也有意义 —— 工具的**顺序**执行是产品契约）。
    if (JSON.stringify(s.tools) !== JSON.stringify(expect.tools)) {
      failures.push(
        `期望工具序列 ${JSON.stringify(expect.tools)}，实际 ${JSON.stringify(s.tools)}`,
      );
    }
  }
  if (Array.isArray(expect.any_of_tools) && expect.any_of_tools.length > 0) {
    // 宽松档：模型可能选不同工具达成同一目标（glob 或 grep 都能找文件）。
    // 只要用了其中一把就算过 —— 但**必须**至少用一把。
    if (!s.tools.some((t) => expect.any_of_tools.includes(t))) {
      failures.push(
        `期望至少用到 ${JSON.stringify(expect.any_of_tools)} 之一，实际 ${JSON.stringify(s.tools)}`,
      );
    }
  }
  if (Array.isArray(expect.files)) {
    const found = await listFiles(profileDir);
    for (const want of expect.files) {
      if (!found.includes(want)) {
        failures.push(`缺少产物 ${want}（实际 ${found.length} 个文件）`);
      }
    }
  }
  if (typeof expect.text_contains === "string" && !s.text.includes(expect.text_contains)) {
    failures.push(`回复里缺 ${JSON.stringify(expect.text_contains)}`);
  }
  if (expect.no_errors === true && s.errors.length > 0) {
    failures.push(`期望零 error 事件，实际：${s.errors.slice(0, 3).join(" / ")}`);
  }
  return { ok: failures.length === 0, failures, summary: s };
}

async function main() {
  const all = JSON.parse(await readFile(casesFile, "utf8"));
  const picked = only ? all.cases.filter((c) => c.name === only) : all.cases;
  if (picked.length === 0) {
    console.error(only ? `没有名为 ${only} 的用例` : "案例集是空的");
    process.exit(2);
  }
  const fp = await fingerprint();

  const baseUrl = all.endpoint?.base_url ?? "http://127.0.0.1:11434/v1";
  const model = all.endpoint?.model ?? null;
  // 「这条是会话层吗」只看用例**自身**的声明，与命令行开关无关 —— 开关决定的是
  // 「跑不跑」，不是「算什么」。把两者混在一起会让 `--boot-only` 把会话用例静默
  // 降级成「起个 GUI 看它活不活」，那既不是它们要验的东西，还会给出一个假绿灯。
  const isSessionCase = (c) =>
    c.boot_only !== true && typeof c.prompt === "string" && c.prompt.length > 0;
  const skipped = bootOnly ? picked.filter(isSessionCase) : [];
  // boot-only 时**剔除**会话用例（而不是降级），剩下的才是这一轮要跑的。
  const cases = bootOnly ? picked.filter((c) => !isSessionCase(c)) : picked;
  if (cases.length === 0) {
    console.error(
      `--boot-only 下没有可跑的启动层用例` +
        (only ? `（${only} 是会话层用例，去掉 --boot-only 再跑）` : ""),
    );
    process.exit(2);
  }
  const sessionCases = cases.filter(isSessionCase);

  console.log(`评测 ${cases.length} 条用例` + (skipped.length > 0 ? `（跳过 ${skipped.length} 条会话层）` : ""));
  console.log(`  代码：${fp.git_sha}${fp.dirty ? "（工作区有未提交改动）" : ""}`);
  console.log(`  版本：${fp.version} · ${fp.platform} · node ${fp.node}`);

  if (dry) {
    for (const c of cases) {
      const kind = isSessionCase(c) ? "会话层" : "启动层";
      console.log(`  - [${kind}] ${c.name}：${c.prompt ?? "（只验冷启动）"}`);
    }
    for (const c of skipped) {
      console.log(`  - [跳过] ${c.name}（--boot-only）`);
    }
    console.log(`\n端点：${baseUrl}${model ? ` · 模型 ${model}` : ""}`);
    if (sessionCases.length > 0) {
      console.log(
        (await endpointAlive(baseUrl))
          ? "  端点可达"
          : "  ⚠ 端点当前不可达 —— 真跑时这一层会明确失败（不会静默跳过）",
      );
    }
    return;
  }

  // 会话层的端点前置检查：不可达就**明确失败**，别跑一堆必然 timeout 的用例。
  if (sessionCases.length > 0 && !(await endpointAlive(baseUrl))) {
    console.error(
      `\n模型端点不可达：${baseUrl}\n` +
        `会话层用例需要真实端点（默认本机 Ollama）。要么先起 Ollama，` +
        `要么用 --boot-only 只跑启动层。`,
    );
    process.exit(1);
  }

  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const runDir = join(here, "runs", stamp);
  await mkdir(runDir, { recursive: true });
  await writeFile(join(runDir, "fingerprint.json"), JSON.stringify(fp, null, 2));

  const results = [];
  for (const c of cases) {
    const profile = join(runDir, "profiles", c.name);
    await rm(profile, { recursive: true, force: true });
    await mkdir(profile, { recursive: true });
    // 用例可以带种子文件/目录（验产物断言的那几条要用）。
    if (c.seed) await cp(join(here, c.seed), profile, { recursive: true });

    if (isSessionCase(c)) {
      console.log(`\n▶ ${c.name}：${c.prompt}`);
      try {
        const r = await runHeadless({
          prompt: c.prompt,
          profileDir: profile,
          session: c.session ?? c.name,
          baseUrl,
          model,
          timeoutSecs: c.timeout_secs ?? 180,
        });
        // 原始证据落盘：出了分歧要能回看这一轮到底发生了什么。
        await writeFile(join(runDir, `${c.name}.ndjson`), r.out);
        if (r.err) await writeFile(join(runDir, `${c.name}.stderr.txt`), r.err);
        const verdict = await judge(c, r, profile);
        console.log(
          verdict.ok
            ? `  ✔ ${c.name}（${r.events.length} 事件，binary=${r.binary}）`
            : `  ✘ ${c.name}\n    ${verdict.failures.join("\n    ")}`,
        );
        results.push({
          name: c.name,
          layer: "session",
          ok: verdict.ok,
          failures: verdict.failures,
          summary: verdict.summary,
          exit_code: r.code,
          binary: r.binary,
          events: r.events.length,
        });
      } catch (e) {
        console.log(`  ✘ ${c.name}\n    ${String(e)}`);
        results.push({ name: c.name, layer: "session", ok: false, failures: [String(e)] });
      }
      continue;
    }

    // 启动层：应用起得来、保持运行、退出后进程树回收。
    console.log(`\n▶ ${c.name}：启动层`);
    let child;
    try {
      child = await startApp(profile);
      await new Promise((r) => setTimeout(r, c.settle_ms ?? 4000));
      const alive = child.exitCode === null;
      console.log(alive ? `  ✔ ${c.name} 已启动` : `  ✘ ${c.name} 启动后立即退出`);
      results.push({
        name: c.name,
        layer: "boot",
        ok: alive,
        failures: alive ? [] : ["应用启动后立即退出，检查日志"],
        exit_code: child.exitCode,
      });
    } catch (e) {
      console.log(`  ✘ ${c.name}\n    ${String(e)}`);
      results.push({ name: c.name, layer: "boot", ok: false, failures: [String(e)] });
    } finally {
      if (child && child.exitCode === null) {
        if (process.platform === "win32") {
          await run("taskkill", ["/PID", String(child.pid), "/T", "/F"]);
        } else {
          try {
            process.kill(-child.pid, "SIGKILL");
          } catch {
            /* 已退出 */
          }
        }
      }
    }
  }

  const passed = results.filter((r) => r.ok).length;
  await writeFile(join(runDir, "results.json"), JSON.stringify(results, null, 2));
  const byLayer = (l) => results.filter((r) => r.layer === l);
  console.log(
    `\n结果：${passed}/${results.length} 通过` +
      `（会话层 ${byLayer("session").filter((r) => r.ok).length}/${byLayer("session").length}，` +
      `启动层 ${byLayer("boot").filter((r) => r.ok).length}/${byLayer("boot").length}）`,
  );
  console.log(`证据：${runDir}`);
  if (passed !== results.length) process.exit(1);
}

main().catch((e) => {
  console.error("评测启动器失败：", e);
  process.exit(2);
});
