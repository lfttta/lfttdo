import type { ContentBlock } from "../types";

/**
 * 一条消息最多带几张图。源头是这份界面给用户的承诺，所以只有这一处定义；
 * 提示文案里的数字也从这里拼，避免「文案说 4、代码放 5」这种第二套真相。
 */
export const MAX_IMAGES_PER_MESSAGE = 4;

/** 一张**已归一化、待发**的图（Step 2 第 3 条：`name` / 预览 / 回来的块）。 */
export interface PendingImage {
  name: string;
  /**
   * `data:` URL。这里**不用** `URL.createObjectURL`：
   * ① jsdom 30 没有实现它（实测 `typeof URL.createObjectURL === "undefined"`），
   *    任何依赖它的测试都要在测试环境里造假；
   * ② blob URL 必须逐个 `revokeObjectURL`，移除一张图就多一条泄漏路径；
   * ③ 更重要：预览取的是 `normalize_image` **回来**的那份字节，所以画出来的就是将要
   *    发出去的那张图（降质、缩放后的样子），而不是用户电脑上的原图。
   * `ImageMime` 的两支（`png`/`jpeg`）正好是 `image/…` 的后缀，所以不需要映射表。
   */
  previewUrl: string;
  /**
   * `normalize_image` 回来的那一支。类型仍是宽 `ContentBlock`（线格式的四支联合，
   * `types.ts:62`），由 `Composer` 收窄一次才进 `SendBlock[]` —— 见 Composer 里那条
   * 「回来的不是 image 支」的用例。
   */
  block: Extract<ContentBlock, { type: "image" }>;
}

export interface ImageStripProps {
  images: PendingImage[];
  /** 移除第 `index` 张（待发列表的唯一删除入口）。 */
  onRemove: (index: number) => void;
  /** 选择器入口交回的 `File`，汇入 `Composer` 的同一个 `addFiles`（R144）。 */
  onPick: (files: File[]) => void;
  /** 逐文件失败的原文行（`<文件名>：<Rust 原文>`），由 Composer 拼好后交下来渲染。 */
  errors: string[];
  /** 最近这一批里被数量上限挡掉的张数（0 表示不显示提示）。 */
  ignored: number;
  /** `streaming` 时禁掉入口：半途改图会让发出去的图和界面看到的不是同一批。 */
  disabled: boolean;
}

/**
 * 待发图条：缩略图 + 数量提示 + 逐文件错误 + 第三个入口（`<input type="file">`）。
 *
 * 它**不自己**收 `File`、不自己调 `normalize`、不持有 `pending` —— 那些都在 `Composer`
 * 的 `addFiles` 里（三个入口汇成一条路径）。这一层只负责「已经发生的事怎么让人看见」，
 * 所以它可以被直接渲染测试（R146）。
 */
export function ImageStrip({
  images,
  onRemove,
  onPick,
  errors,
  ignored,
  disabled,
}: ImageStripProps) {
  return (
    <div className="image-strip" data-testid="image-strip">
      {images.length > 0 ? (
        <ul className="image-list">
          {images.map((img, i) => (
            <li className="image-item" key={`${img.name}-${i}`}>
              <img
                className="image-thumb"
                data-testid={`thumb-${i}`}
                src={img.previewUrl}
                alt={img.name}
              />
              <span className="image-name">{img.name}</span>
              <button
                type="button"
                className="image-remove"
                disabled={disabled}
                aria-label={`移除 ${img.name}`}
                onClick={() => onRemove(i)}
              >
                移除
              </button>
            </li>
          ))}
        </ul>
      ) : null}

      {ignored > 0 ? (
        <p className="image-note" data-testid="image-overflow">
          一次最多 {MAX_IMAGES_PER_MESSAGE} 张，已忽略 {ignored} 张
        </p>
      ) : null}

      {errors.length > 0 ? (
        <ul className="image-errors">
          {errors.map((e, i) => (
            <li className="image-error" data-testid="image-error" key={`${e}-${i}`}>
              {e}
            </li>
          ))}
        </ul>
      ) : null}

      {/* R144：第三个入口是 `<input type="file">`，不是 dialog 返回的路径 ——
          capabilities 只有 `dialog:allow-open`、没有任何 fs 读权限，拿到路径也读不出字节，
          而为它放权直接违反「前端不得有文件/执行能力」这条红线。
          入口**还是那个 input**（testid、change 合同、「清 value 允许重选同一文件」都不变），
          2026-11-08 换成「label + 视觉隐藏的 input」：可见的那枚「添加图片」是普通 DOM，
          文案归我们 —— 之前是给 UA 按钮做皮肤，而「选择文件」那串字是引擎画的，
          `content` 打不进去（styles.css 旧注释实测过），它成了输入区里唯一一块
          系统默认模板的残留。键盘 Tab 到 input 回车、或点 label 任意处，都开同一个选择器。 */}
      <label className="image-file-entry">
        <span className="image-file-button">添加图片</span>
        <input
          className="image-file-input"
          data-testid="file-input"
          type="file"
          multiple
          accept="image/*"
          disabled={disabled}
          aria-label="添加图片"
          onChange={(e) => {
            const files = Array.from(e.target.files ?? []);
            // 不清 value 的话，连续选取**同一个**文件不会触发 change（R144 的连带要求）。
            e.target.value = "";
            onPick(files);
          }}
        />
      </label>
    </div>
  );
}
