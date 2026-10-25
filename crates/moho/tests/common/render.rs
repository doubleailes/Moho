//! The render settings fixture shared by the render tests.

pub const SCENE: &str = r#"#usda 1.0
(
    renderSettingsPrimPath = "/Render/settings"
)
def Camera "cam"
{
}
def Camera "cam2"
{
}
def Scope "Render"
{
    def RenderSettings "settings"
    {
        rel camera = </cam>
        int2 resolution = (1920, 1080)
        uniform token renderingColorSpace = "acescg"
        int crust:samplesPerPixel = 64
        rel products = [</Render/beauty>, </Render/aovs>]
    }
    def RenderProduct "beauty"
    {
        token productName = "beauty.exr"
        rel orderedVars = [</Render/color>, </Render/depth>]
        string driver:parameters:artist = "phil"
    }
    def RenderProduct "aovs"
    {
        rel camera = </cam2>
        int2 resolution = (960, 540)
        token productName = "aovs.exr"
        rel orderedVars = </Render/normal>
    }
    def RenderVar "color"
    {
        token dataType = "color3f"
        string sourceName = "Ci"
    }
    def RenderVar "depth"
    {
        token dataType = "float"
        string sourceName = "z"
    }
    def RenderVar "normal"
    {
        token dataType = "normal3f"
        string sourceName = "N"
    }
    def RenderPass "pass"
    {
        token passType = "prman"
        rel renderSource = </Render/settings>
        rel collection:matte:includes = </cam>
    }
}
"#;
