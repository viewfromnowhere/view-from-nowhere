use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Levels of stealth applied to the browser session.
pub enum StealthProfile {
    Lightweight,
    Balanced,
    Maximum,
}

/// Construct Chrome command‑line arguments for a given stealth profile
/// and fingerprint.
pub fn build_stealth_arguments(
    profile: &StealthProfile,
    user_profile: &super::fingerprint::UserAgentProfile,
) -> Vec<String> {
    let mut args = vec![
        "--disable-blink-features=AutomationControlled".to_string(),
        "--disable-infobars".to_string(),
        "--disable-dev-shm-usage".to_string(),
        "--no-sandbox".to_string(),
        "--disable-extensions-file-access-check".to_string(),
        "--disable-extensions".to_string(),
        "--disable-plugins-discovery".to_string(),
        // FIXME(security): `--disable-web-security` weakens browser isolation and
        // may violate site expectations/TOS. Consider gating behind a feature flag
        // or restricting to `Maximum` profile only.
        "--disable-web-security".to_string(),
        format!("--user-agent={}", user_profile.user_agent),
        format!(
            "--window-size={},{}",
            user_profile.viewport.0, user_profile.viewport.1
        ),
        format!("--lang={}", user_profile.languages.join(",")),
    ];
    if let StealthProfile::Maximum = profile {
        args.push("--disable-gpu".to_string());
    }
    args
}

/// JavaScript evasions applied at page load to reduce automation signals.
pub struct StealthScripts;

impl StealthScripts {
    pub fn get_core_evasions() -> &'static str {
        r#"
            Object.defineProperty(navigator, 'webdriver', { get: () => undefined });
            Object.defineProperty(navigator, 'plugins', { get: () => [1,2,3] });
            Object.defineProperty(navigator, 'languages', {
                get: () => ['en-US', 'en']
            });
            if (!window.chrome) window.chrome = { runtime: {} };
        "#
    }
    pub fn get_webgl_evasions() -> &'static str {
        r#"
            const getParameter = WebGLRenderingContext.prototype.getParameter;
            WebGLRenderingContext.prototype.getParameter = function(parameter) {
                if (parameter === 37445) return 'Intel Inc.';
                if (parameter === 37446) return 'Intel Iris OpenGL Engine';
                return getParameter.call(this, parameter);
            };
        "#
    }
    pub fn get_canvas_evasions() -> &'static str {
        r#"
            const getContext = HTMLCanvasElement.prototype.getContext;
            HTMLCanvasElement.prototype.getContext = function(type,...args){
                const ctx = getContext.call(this,type,...args);
                if(type==='2d' && ctx) {
                    const origToDataURL=this.toDataURL;
                    this.toDataURL=function(...a){
                        const imgdata=ctx.getImageData(0,0,this.width,this.height);
                        for(let i=0;i<imgdata.data.length;i+=4){
                            if(Math.random()<0.001)imgdata.data[i]+=Math.random()<0.5?-1:1;
                        }
                        ctx.putImageData(imgdata,0,0);
                        return origToDataURL.call(this,...a);
                    };
                }
                return ctx;
            };
        "#
    }
}
