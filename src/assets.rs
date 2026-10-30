//! Icons beyond gpui-kit's default set, composed with the kit's own assets.

use gpui_kit::{AssetSource, Result, SharedString};
use std::borrow::Cow;

gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        Bot,
        CircleAlert,
        CircleStop,
        Columns2,
        ExternalLink,
        FilePlus,
        FolderPlus,
        GitBranch,
        Lightbulb,
        LoaderCircle,
        Maximize2,
        Minimize2,
        PanelRight,
        Pencil,
        Send,
        Shield,
        Sparkles,
        SquareTerminal,
        Terminal,
        Wrench,
        X,
        ArrowUp,
        Copy,
        Files,
        Square,
        CircleDashed,
        KeyRound,
        Keyboard,
        PanelRightClose,
        Plug,
        Lock,
        ShieldAlert,
        TriangleAlert,
        Expand,
        Pin,
        PinOff,
        Package,
        Link,
        GitMerge,
        GitBranchPlus,
        GitCommitHorizontal,
        RefreshCw,
        Flag,
        Check,
        MoveUpRight,
        ZoomIn,
        ZoomOut,
    ]
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match ExtraIcons.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
