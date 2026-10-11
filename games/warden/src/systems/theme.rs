//! UI theme tokens (design.md 的代码事实源)：晴昼要塞材质三件套——白灰石、
//! 羊皮纸、要塞金。所有 UI 代码从 这里取值，禁散落 magic number。
//! 参见 games/warden/docs/design.md。

use bevy::prelude::*;

// --- 材质色板（sRGB） ----------------------------------------------------------

/// 白灰石：HUD 骨架底色（顶匾 / 底甲板 / 羊皮纸镶片底）。
pub const STONE: Color = Color::srgb(0.659, 0.635, 0.589); // #A8A296
/// 石缝线：石件 1px 描边 / 按钮槽底。
pub const STONE_SEAM: Color = Color::srgb(0.431, 0.412, 0.369); // #6E695E
/// 石亮：顶缘阳光亮线 / 悬停描边。
pub const STONE_LIP: Color = Color::srgb(0.769, 0.745, 0.690); // #C4BEB0
/// 石哑：不可用按钮底。
pub const STONE_DIM: Color = Color::srgb(0.541, 0.518, 0.471); // #8A8478

/// 羊皮纸：信息面底色（军报 / 三选一 / 终局 / 技能小卡）。
pub const PARCHMENT: Color = Color::srgb(0.937, 0.902, 0.816); // #EFE6D0
/// 羊皮纸缝线。
pub const PARCHMENT_SEAM: Color = Color::srgb(0.788, 0.749, 0.651); // #C9BFA6

/// 要塞金：金币数字 / 可用描边 / 主行动槽。
pub const GOLD: Color = Color::srgb(0.788, 0.635, 0.153); // #C9A227
/// 金压暗（金面上的文字 / 金描边的暗侧）。
pub const GOLD_DARK: Color = Color::srgb(0.561, 0.451, 0.094); // #8F7318

/// 墨：石面/羊皮纸上的主文字。
pub const INK: Color = Color::srgb(0.180, 0.165, 0.141); // #2E2A24
/// 亮墨：石面上的次级文字（比墨浅一档，仍守对比）。
pub const INK_SOFT: Color = Color::srgb(0.290, 0.263, 0.220); // #4A4338

/// 警红：低血量 / 买不起的价格。
pub const ALERT_RED: Color = Color::srgb(0.663, 0.196, 0.149); // #A93226
/// 钢蓝：法术系角标 / 技能冷色信息。
pub const STEEL_BLUE: Color = Color::srgb(0.247, 0.380, 0.502); // #3F6180

// --- 字号阶梯（单家族 Noto Sans SC；层级靠字重+字号） --------------------------

pub const FONT_NUM: f32 = 22.0; // 顶匾大数字
pub const FONT_TITLE: f32 = 15.0; // 军报/卡片标题
pub const FONT_BUTTON: f32 = 13.0; // 按钮主字 / 正文
pub const FONT_LABEL: f32 = 11.0; // 标签 / 提示
pub const FONT_BADGE: f32 = 9.0; // 快捷键角标

// --- 尺寸与间距（4 的倍数） -----------------------------------------------------

pub const BAR_HEIGHT: f32 = 128.0; // 底甲板
pub const BTN_W: f32 = 108.0; // 命令卡石槽
pub const BTN_H: f32 = 46.0;
pub const GAP: f32 = 8.0; // 通用间距
pub const CARD_W: f32 = 200.0; // 三选一羊皮纸卡
pub const CARD_H: f32 = 88.0;

/// 石面板的标准做法（无 9-slice 资产的过渡态）：石底 + 1px 缝线边框。
pub fn stone_node() -> Node {
    Node {
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    }
}

/// 羊皮纸面板：纸底 + 3px 微圆角 + 缝线边框。
pub fn parchment_node() -> Node {
    Node {
        border: UiRect::all(Val::Px(1.0)),
        border_radius: BorderRadius::all(Val::Px(3.0)),
        ..default()
    }
}
