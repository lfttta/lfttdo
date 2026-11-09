#!/usr/bin/env node
// G45：会话级回归评测的**外部启动器**。
//
// 责任边界（照 ZCode usage/13 的取向）：LFTTDO 只当执行器 —— 案例集、环境隔离、
// 资源预算、评分全部在这里（仓库外的脚本），产品代码不为评测加任何开关。
//
// 用法：
//   node scripts/eval/run.mjs                 # 跑全部用例
//   node scripts/eval/run.mjs --case grep-rs   # 只跑一条
//   node scripts/eval/run.mjs --dry            # 只打印将要做的事（不启动应用）
//
// 这里跑的是**用真实端点**的端到端会话（默认本机 Ollama），因此：
//   · 不进 CI（CI 无模型、无网络）；
//   · 每次运行的证据落在 `scripts/eval/runs/<时间戳>/`（不提交，`.gitignore` 已覆盖）；
//   · 每个用例的判定是「期望的工具序列 + 产物断言」，不是「文本像不像」——
//     模型措辞天天变，行为契约才是回归信号。

import { spawn } from "node:child_process";
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "..", "..");
const casesFile = join(here, "cases.json");

const argv = process.argv.slice(2);
const only = argv.includes("--case") ? argv[argv.indexOf("--case") + 1] : null;
const dry = argv.includes("--dry");

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
    child.stdout?.on("data", (d) => (out += d));
    child.stderr?.on("data", (d) => (err += d));
    const timer = setTimeout(() => {
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
      res({ code, out, err, timedOut: false });
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

/**
 * 起应用。评测用的是**发布版二进制**（`npm run tauri build` 的产物）而不是
 * dev server：dev 会拉一个 Vite 端口、还带热重载，评测要的是确定的东西。
 * 二进制不存在就明确失败，不偷偷降级成 dev。
 */
async function startApp(profileDir) {
  const exe =
    process.platform === "win32"
      ? join(repo, "src-tauri", "target", "release", "lfttdo.exe")
      : join(repo, "src-tauri", "target", "release", "lfttdo");
  if (!existsSync(exe)) {
    throw new Error(
      `找不到发布版二进制：${exe}\n先跑 \`npm run tauri build\`（评测不替你构建）。`,
    );
  }
  // 隔离：独立数据目录 + 独立 HOME，评测不碰用户真实配置/会话/记忆。
  const env = {
    HOME: profileDir,
    USERPROFILE: profileDir,
    APPDATA: join(profileDir, "roaming"),
    LOCALAPPDATA: join(profileDir, "local"),
  };
  const child = spawn(exe, [], {
    cwd: profileDir,
    env: { ...process.env, ...env },
    windowsHide: false,
    detached: process.platform !== "win32",
  });
  return child;
}

function summarize(evts) {
  const toolNames = evts
    .filter((e) => e.t === "tool_call")
    .map((e) => e.call?.name ?? "?");
  return {
    tool_names: toolNames,
    stop_reason: evts.find((e) => e.t === "done")?.stop_reason ?? null,
    errors: evts.filter((e) => e.t === "error").map((e) => e.message),
  };
}

async function main() {
  const all = JSON.parse(await readFile(casesFile, "utf8"));
  const cases = only ? all.cases.filter((c) => c.name === only) : all.cases;
  if (cases.length === 0) {
    console.error(only ? `没有名为 ${only} 的用例` : "案例集是空的");
    process.exit(2);
  }
  const fp = await fingerprint();
  console.log(`评测 ${cases.length} 条用例`);
  console.log(`  代码：${fp.git_sha}${fp.dirty ? "（工作区有未提交改动）" : ""}`);
  console.log(`  版本：${fp.version} · ${fp.platform} · node ${fp.node}`);
  if (dry) {
    for (const c of cases) console.log(`  - ${c.name}：${c.prompt}`);
    return;
  }

  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const runDir = join(here, "runs", stamp);
  await mkdir(runDir, { recursive: true });
  await writeFile(join(runDir, "fingerprint.json"), JSON.stringify(fp, null, 2));

  const results = [];
  for (const c of cases) {
    console.log(`\n▶ ${c.name}：${c.prompt}`);
    const profile = join(runDir, "profiles", c.name);
    await rm(profile, { recursive: true, force: true });
    await mkdir(profile, { recursive: true });
    // 用例可以带种子文件/目录（验产物断言的那几条要用）。
    if (c.seed) await cp(join(here, c.seed), profile, { recursive: true });

    let child;
    try {
      child = await startApp(profile);
      // 应用是 GUI：这里**不驱动界面**（评测不模拟点击，界面回归属另一层）。
      // 本轮把「起得来 + 数据目录隔离 + 指纹归档」跑通；模型驱动的会话断言
      // 需要 headless 入口（计划里记为 G45 的下一步），不在这一版假装做到。
      await new Promise((r) => setTimeout(r, c.settle_ms ?? 4000));
      const alive = child.exitCode === null;
      results.push({
        name: c.name,
        status: alive ? "started" : "exited-early",
        exit_code: child.exitCode,
        note: alive
          ? "应用已启动并保持运行（会话级断言待 headless 入口，见计划）"
          : "应用启动后立即退出，检查日志",
      });
    } catch (e) {
      results.push({ name: c.name, status: "error", error: String(e) });
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

  const passed = results.filter((r) => r.status === "started").length;
  await writeFile(join(runDir, "results.json"), JSON.stringify(results, null, 2));
  console.log(`\n结果：${passed}/${results.length} 起得来`);
  console.log(`证据：${runDir}`);
  if (passed !== results.length) process.exit(1);
}

main().catch((e) => {
  console.error("评测启动器失败：", e);
  process.exit(2);
});
