//! The HTML and SVG attributes `jsx!` knows on intrinsic elements (React 17's own list, from
//! `react-dom`'s `possibleStandardNames`), its event handler props, and what each one takes.

/// What an attribute of an HTML element takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    /// `on_click={..}`: a closure of the event (`js::Event`), of nothing, or of a `JsValue`.
    Handler,
    /// `class`, `id`, `href`, ..: text.
    Text,
    /// `width`, `tab_index`, `value`, most SVG attributes: a number or text.
    TextOrNum,
    /// `disabled`, `checked`, ..: a `bool`.
    Bool,
    /// `style`: a CSS string or `(name, value)` pairs.
    Style,
    /// `key`: text or a number.
    Key,
    /// `ref={r}`: a `react::Ref`, set to the element.
    Ref,
    /// `data-*`, `aria-*`, `dangerously_set_inner_html`, a custom element's: any value.
    Any,
}

/// React's event handler props (each also has a `..Capture` form).
const EVENTS: &str = "
    onAbort onAnimationEnd onAnimationIteration onAnimationStart onAuxClick onBeforeInput onBlur
    onCanPlay onCanPlayThrough onCancel onChange onClick onClose onCompositionEnd
    onCompositionStart onCompositionUpdate onContextMenu onCopy onCut onDoubleClick onDrag
    onDragEnd onDragEnter onDragExit onDragLeave onDragOver onDragStart onDrop onDurationChange
    onEmptied onEncrypted onEnded onError onFocus onGotPointerCapture onInput onInvalid
    onKeyDown onKeyPress onKeyUp onLoad onLoadStart onLoadedData onLoadedMetadata
    onLostPointerCapture onMouseDown onMouseEnter onMouseLeave onMouseMove onMouseOut
    onMouseOver onMouseUp onPaste onPause onPlay onPlaying onPointerCancel onPointerDown
    onPointerEnter onPointerLeave onPointerMove onPointerOut onPointerOver onPointerUp
    onProgress onRateChange onReset onScroll onSeeked onSeeking onSelect onStalled onSubmit
    onSuspend onTimeUpdate onToggle onTouchCancel onTouchEnd onTouchMove onTouchStart
    onTransitionEnd onVolumeChange onWaiting onWheel
";

/// React's names of the other DOM attributes (HTML and SVG).
const ATTRS: &str = "
    about accentHeight accept acceptCharset accessKey accumulate action additive
    alignmentBaseline allowFullScreen allowReorder alphabetic alt amplitude arabicForm as ascent
    async attributeName attributeType autoCapitalize autoComplete autoCorrect autoFocus autoPlay
    autoReverse autoSave azimuth baseFrequency baseProfile baselineShift bbox begin bias by
    calcMode capHeight capture cellPadding cellSpacing challenge charSet checked cite classID
    clip clipPath clipPathUnits clipRule colSpan color colorInterpolation
    colorInterpolationFilters colorProfile colorRendering cols content contentEditable
    contentScriptType contentStyleType contextMenu controls controlsList coords crossOrigin
    cursor cx cy d data datatype dateTime decelerate default defaultChecked defaultValue defer
    descent diffuseConstant dir direction disablePictureInPicture disableRemotePlayback disabled
    display divisor dominantBaseline download draggable dur dx dy edgeMode elevation
    enableBackground encType end enterKeyHint exponent externalResourcesRequired fill
    fillOpacity fillRule filter filterRes filterUnits floodColor floodOpacity focusable
    fontFamily fontSize fontSizeAdjust fontStretch fontStyle fontVariant fontWeight form
    formAction formEncType formMethod formNoValidate formTarget format frameBorder from fx fy g1
    g2 glyphName glyphOrientationHorizontal glyphOrientationVertical glyphRef gradientTransform
    gradientUnits hanging headers height hidden high horizAdvX horizOriginX href hrefLang
    htmlFor httpEquiv icon id ideographic imageRendering in in2 inlist inputMode integrity
    intercept is itemID itemProp itemRef itemScope itemType k k1 k2 k3 k4 kernelMatrix
    kernelUnitLength kerning keyParams keyPoints keySplines keyTimes keyType kind label lang
    lengthAdjust letterSpacing lightingColor limitingConeAngle list local loop low manifest
    marginHeight marginWidth markerEnd markerHeight markerMid markerStart markerUnits
    markerWidth mask maskContentUnits maskUnits mathematical max maxLength media mediaGroup
    method min minLength mode multiple muted name noModule noValidate nonce numOctaves offset
    opacity open operator optimum order orient orientation origin overflow overlinePosition
    overlineThickness paintOrder panose1 pathLength pattern patternContentUnits patternTransform
    patternUnits placeholder playsInline pointerEvents points pointsAtX pointsAtY pointsAtZ
    poster prefix preload preserveAlpha preserveAspectRatio primitiveUnits profile property r
    radioGroup radius readOnly refX refY referrerPolicy rel renderingIntent repeatCount
    repeatDur required requiredExtensions requiredFeatures resource restart result results
    reversed role rotate rowSpan rows rx ry sandbox scale scope scoped scrolling seamless
    security seed selected shape shapeRendering size sizes slope spacing span specularConstant
    specularExponent speed spellCheck spreadMethod src srcDoc srcLang srcSet start startOffset
    stdDeviation stemh stemv step stitchTiles stopColor stopOpacity strikethroughPosition
    strikethroughThickness string stroke strokeDasharray strokeDashoffset strokeLinecap
    strokeLinejoin strokeMiterlimit strokeOpacity strokeWidth summary
    suppressContentEditableWarning suppressHydrationWarning surfaceScale systemLanguage tabIndex
    tableValues target targetX targetY textAnchor textDecoration textLength textRendering title
    to transform type typeof u1 u2 underlinePosition underlineThickness unicode unicodeBidi
    unicodeRange unitsPerEm unselectable useMap vAlphabetic vHanging vIdeographic vMathematical
    value values vectorEffect version vertAdvY vertOriginX vertOriginY viewBox viewTarget
    visibility vocab width widths wmode wordSpacing wrap writingMode x x1 x2 xChannelSelector
    xHeight xlinkActuate xlinkArcrole xlinkHref xlinkRole xlinkShow xlinkTitle xlinkType xmlBase
    xmlLang xmlSpace xmlns xmlnsXlink y y1 y2 yChannelSelector z zoomAndPan
";

/// The attributes that take a `bool`.
const BOOLS: &str = "allowFullScreen async autoFocus autoPlay checked controls default defaultChecked
    defer disabled disablePictureInPicture disableRemotePlayback formNoValidate hidden itemScope loop
    multiple muted noModule noValidate open playsInline readOnly required reversed scoped seamless
    selected suppressContentEditableWarning suppressHydrationWarning";

/// The attributes that take text (others take a number or text).
const TEXTS: &str = "about accept acceptCharset accessKey action alt as autoCapitalize autoComplete
    autoCorrect autoSave charSet cite classID className content contextMenu controlsList crossOrigin
    dateTime dir download encType enterKeyHint form formAction formEncType formMethod formTarget
    headers href hrefLang htmlFor httpEquiv icon id inputMode integrity is itemID itemProp itemRef
    itemType keyParams keyType kind label lang list manifest media mediaGroup method name nonce
    pattern placeholder poster prefix preload property radioGroup referrerPolicy rel resource role
    sandbox scope security shape sizes slot src srcDoc srcLang srcSet target title type useMap
    vocab wmode wrap";

/// HTML's elements, and the common SVG ones (for completions after `<`, and to read `<input>` as
/// the element where a local is named `input`).
pub(crate) const HTML_TAGS: &str = "a abbr address area article aside audio b bdi bdo blockquote br
    button canvas caption cite code col colgroup data datalist dd del details dfn dialog div dl dt
    em embed fieldset figcaption figure footer form h1 h2 h3 h4 h5 h6 header hr i iframe img input
    ins kbd label legend li main map mark menu meter nav object ol optgroup option output p param
    picture pre progress q rp rt ruby s samp section select small source span strong sub summary
    sup table tbody td template textarea tfoot th thead time tr track u ul var video wbr svg g path
    circle ellipse line polyline polygon rect text tspan defs linearGradient radialGradient stop";

/// Whether `tag` is one of `HTML_TAGS`.
pub(crate) fn is_html_tag(tag: &str) -> bool {
    has(HTML_TAGS, tag)
}

fn has(list: &str, name: &str) -> bool {
    list.split_whitespace().any(|n| n == name)
}

/// The React event names (`onClick`), without the `..Capture` forms.
pub(crate) fn events() -> impl Iterator<Item = &'static str> {
    EVENTS.split_whitespace()
}

/// React's attribute names of intrinsic elements, for suggestions and completions.
pub(crate) fn names() -> impl Iterator<Item = &'static str> {
    [
        "className",
        "style",
        "key",
        "ref",
        "dangerouslySetInnerHTML",
    ]
    .into_iter()
    .chain(ATTRS.split_whitespace())
}

/// What attribute `js` (React's name) of `<tag>` takes; `None` for one the element doesn't have.
pub(crate) fn kind(tag: &str, js: &str) -> Option<Kind> {
    if js.starts_with("on") && js[2..].starts_with(|c: char| c.is_ascii_uppercase()) {
        let base = js.strip_suffix("Capture").unwrap_or(js);
        if has(EVENTS, js) || has(EVENTS, base) || tag.contains('-') {
            return Some(Kind::Handler);
        }
        return None;
    }
    Some(match js {
        "key" => Kind::Key,
        "ref" => Kind::Ref,
        "style" => Kind::Style,
        "dangerouslySetInnerHTML" => Kind::Any,
        _ if js.starts_with("data-") || js.starts_with("aria-") || tag.contains('-') => Kind::Any,
        _ if has(BOOLS, js) => Kind::Bool,
        _ if has(TEXTS, js) || js == "className" => Kind::Text,
        _ if has(ATTRS, js) => Kind::TextOrNum,
        _ => return None,
    })
}
