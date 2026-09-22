package io.github.yurisismotto.omnibridge.ui.components

import androidx.annotation.DrawableRes
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.omnibridge.R
import io.github.yurisismotto.omnibridge.ui.UiMapping
import io.github.yurisismotto.omnibridge.ui.theme.Brand
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeGradient
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeIconSize
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeRadius
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeSpacing
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeStatus
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeTheme
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeType

/**
 * The exchange-flow family: the screens whose whole purpose is moving one
 * thing from here to one named computer.
 *
 * ## What this is, and what it deliberately is not
 *
 * Two screens share it — Send clipboard and the Sharesheet's Send files —
 * and they share it because they are the same act with a different payload:
 * *this thing*, *to that computer*, *over this link*. Everything here exists
 * to stop those two drifting apart, and nothing here exists for any other
 * screen. Devices, Settings, notification consent and the Files list are not
 * exchanges: they are places, and a place that borrows the vocabulary of a
 * send would be claiming to be one.
 *
 * So this is four small pieces and one background, not a framework:
 *
 *  * [ExchangeHero] — what is moving, and roughly where to;
 *  * [ExchangePeerStatus] — the named destination and the live link to it;
 *  * [ExchangePayloadCard] / [ExchangePayloadEmpty] — exactly what is going;
 *  * [ExchangeSecurityFooter] — what is true about the link.
 *
 * ## Everything dynamic comes from state
 *
 * Not one string or glyph here is a stand-in. The peer's name, the platform
 * line and the device glyph are passed in by the screen from live session
 * state, and the device glyph in particular falls back to a neutral one
 * rather than a monitor when nothing has said what the peer runs — see
 * [UiMapping.peerDeviceKind] and [deviceKindIcon]. A picture is a claim.
 */

/** The size of one hero endpoint tile. */
private val TileSize: Dp = 64.dp

/**
 * The span the flow graphic occupies between the two tiles.
 *
 * Sized from the reference rather than chosen: there the tile is 160 units
 * and the flow 235 by 110 in the same picture, so against a 64dp tile they
 * are these. Getting that ratio wrong is what made the first attempt read as
 * two icons with a scribble between them — the ribbons need the length to
 * carry a wave.
 */
private val FlowWidth: Dp = 94.dp
private val FlowHeight: Dp = 44.dp

/**
 * The glyph for a device kind.
 *
 * A `when` over [UiMapping.DeviceKind] rather than a drawable id stored on
 * the enum: resource ids belong to the UI layer, and [UiMapping] stays a
 * plain Kotlin object a JVM test can call without an Android context — the
 * same division [io.github.yurisismotto.omnibridge.ui.FilesMapping] keeps.
 *
 * [UiMapping.DeviceKind.Unknown] gets its own neutral glyph. Drawing a
 * monitor for a peer that has never said what it runs would be inventing the
 * platform in pictures, which is exactly the defect the identity line was
 * fixed for.
 */
@DrawableRes
fun deviceKindIcon(kind: UiMapping.DeviceKind): Int = when (kind) {
    UiMapping.DeviceKind.Desktop -> R.drawable.ic_device_desktop
    UiMapping.DeviceKind.Mobile -> R.drawable.ic_device_phone
    UiMapping.DeviceKind.Unknown -> R.drawable.ic_device_generic
}

/**
 * Source, flow, destination — the one picture an exchange screen gets.
 *
 * Entirely decorative and hidden from assistive technology: the peer pill
 * below it names the destination in words, and artwork must never be the
 * only place a fact appears.
 *
 * Fixed sizes rather than weights on purpose. Stretched across a tablet the
 * same composition becomes two small tiles a hand's width apart with a
 * flattened ribbon between them, which is the "pulled phone layout" the
 * content column exists to prevent.
 *
 * @param sourceIcon what is being sent — a clipboard, a folder.
 * @param destinationIcon the peer's own device glyph, from [deviceKindIcon].
 */
@Composable
fun ExchangeHero(
    @DrawableRes sourceIcon: Int,
    @DrawableRes destinationIcon: Int,
    modifier: Modifier = Modifier,
) {
    val colors = OmniBridgeTheme.colors
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = OmniBridgeSpacing.lg)
            .decorative(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        ExchangeTile(sourceIcon, colors.accentCyan)
        ExchangeFlow(
            Modifier
                .padding(horizontal = OmniBridgeSpacing.xxs)
                .width(FlowWidth)
                .height(FlowHeight),
        )
        ExchangeTile(destinationIcon, colors.accentBlue)
    }
}

/**
 * One end of the hero: a soft tinted tile with a glyph in it.
 *
 * Both ends are drawn the same way. The payload and the computer are the two
 * things being joined, and giving one a tile and the other a bare icon makes
 * them read as a label and a destination rather than as two ends of one span.
 *
 * The tint is the accent at low alpha, so it is a wash rather than a block of
 * colour, and the glyph on it is the AA-corrected accent — the tile is
 * decoration, the glyph is the thing you have to be able to see.
 */
@Composable
private fun ExchangeTile(@DrawableRes icon: Int, accent: Color) {
    val tint = accent.copy(alpha = if (OmniBridgeTheme.colors.isDark) 0.22f else 0.10f)
    Box(
        Modifier
            .size(TileSize)
            .clip(RoundedCornerShape(OmniBridgeRadius.xlarge))
            .background(tint),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painter = painterResource(icon),
            contentDescription = null,
            tint = accent,
            modifier = Modifier.size(OmniBridgeIconSize.xlarge),
        )
    }
}

/**
 * The bridge between the two tiles: two ribbons and four motes.
 *
 * ## The shape is measured, not invented
 *
 * The curves, the weights, the four motes and the fades are traced from the
 * approved reference: two ribbons carrying the same wave a third of a phase
 * apart, the near one starting at a solid teal terminal and fading out as it
 * falls away to the right, the far one entering almost invisibly and
 * gathering into a solid violet terminal at the destination. That asymmetry
 * is what gives the picture a direction — the ink arrives where the content
 * is going.
 *
 * Both paths run strictly left to right. Neither loops, doubles back or
 * crosses the other: a crossing would draw an infinity mark and a return
 * would draw a squiggle, and neither means "from here to there".
 *
 * ## The colours are the brand tokens, at the reference's weights
 *
 * The reference's own ink is a shade minter than [Brand.Cyan] and a shade
 * paler than [Brand.Violet]. The hues here are the tokens; what is copied
 * from the reference is the *opacity ramp* along each ribbon, which is what
 * makes the picture read as light and airy rather than as two painted
 * cables. Nothing is ever drawn on top of this, so the vivid identity
 * gradient is the right one — see [OmniBridgeGradient].
 *
 * Drawn, not rasterised: it costs two strokes and four circles a frame and
 * scales to any density.
 */
@Composable
fun ExchangeFlow(modifier: Modifier = Modifier) {
    val dark = OmniBridgeTheme.colors.isDark
    // On a dark surface the same low opacities all but vanish. They are
    // lifted a little rather than the hues being brightened, so the ribbons
    // stay a soft background presence instead of glowing.
    val lift = if (dark) 1.3f else 1f
    fun Color.at(alpha: Float) = copy(alpha = (alpha * lift).coerceAtMost(1f))

    Canvas(modifier.decorative()) {
        val w = size.width
        val h = size.height

        // The near ribbon: out of the teal terminal, up over a shallow crest,
        // then away to the lower right, thinning into the page as it goes.
        val lead = Path().apply {
            moveTo(w * 0.038f, h * 0.600f)
            cubicTo(w * 0.105f, h * 0.520f, w * 0.165f, h * 0.395f, w * 0.260f, h * 0.395f)
            cubicTo(w * 0.400f, h * 0.400f, w * 0.680f, h * 0.915f, w * 0.870f, h * 0.900f)
        }
        // The far ribbon: the same wave a third of a phase along, entering
        // faint at the top and gathering into the violet terminal.
        val trail = Path().apply {
            moveTo(w * 0.210f, h * 0.150f)
            cubicTo(w * 0.270f, h * 0.100f, w * 0.420f, h * 0.115f, w * 0.583f, h * 0.314f)
            cubicTo(w * 0.660f, h * 0.470f, w * 0.800f, h * 0.680f, w * 0.949f, h * 0.464f)
        }

        drawPath(
            path = lead,
            brush = Brush.linearGradient(
                colorStops = arrayOf(
                    // The stops are placed where the ribbon actually is, not
                    // spread over the whole box: a ramp that has not reached
                    // zero by the path's last point leaves a visible round cap
                    // hanging in the middle of the picture.
                    0.00f to Brand.Cyan.at(0.95f),
                    0.26f to Brand.Cyan.at(0.55f),
                    0.55f to Brand.Blue.at(0.30f),
                    0.78f to Brand.Blue.at(0.10f),
                    0.90f to Brand.Blue.copy(alpha = 0f),
                    1.00f to Brand.Blue.copy(alpha = 0f),
                ),
                start = Offset(0f, 0f),
                end = Offset(w, 0f),
            ),
            style = Stroke(width = h * 0.10f, cap = StrokeCap.Round),
        )
        drawPath(
            path = trail,
            brush = Brush.linearGradient(
                colorStops = arrayOf(
                    0.00f to Brand.Blue.copy(alpha = 0f),
                    0.21f to Brand.Blue.copy(alpha = 0f),
                    0.40f to Brand.Blue.at(0.20f),
                    0.65f to Brand.Violet.at(0.42f),
                    0.85f to Brand.Violet.at(0.68f),
                    1.00f to Brand.Violet.at(0.90f),
                ),
                start = Offset(0f, 0f),
                end = Offset(w, 0f),
            ),
            style = Stroke(width = h * 0.11f, cap = StrokeCap.Round),
        )

        // Two terminals and two motes. The terminals are the ends of the two
        // ribbons and are what make the flow start and finish somewhere; the
        // motes travel between them, off the paths so that neither can be
        // mistaken for a point on a plotted line.
        drawCircle(
            Brand.Cyan.at(0.95f),
            radius = h * 0.082f,
            center = Offset(w * 0.038f, h * 0.600f),
        )
        drawCircle(
            // The hue the ribbons are wearing where this mote sits: a third
            // of the way from cyan towards blue, rather than a fifth colour.
            lerp(Brand.Cyan, Brand.Blue, 0.35f).at(0.52f),
            radius = h * 0.073f,
            center = Offset(w * 0.319f, h * 0.718f),
        )
        drawCircle(
            Brand.Violet.at(0.55f),
            radius = h * 0.073f,
            center = Offset(w * 0.719f, h * 0.290f),
        )
        drawCircle(
            Brand.Violet.at(0.85f),
            radius = h * 0.082f,
            center = Offset(w * 0.949f, h * 0.464f),
        )
    }
}

/**
 * The destination, in words: which computer, and what is known about it.
 *
 * Every value is live. [label] is built by the screen from the peer's own
 * name, [identity] is [UiMapping.peerIdentityLine] — the platform while a
 * session is up and the pinned fingerprint otherwise — and the tint comes
 * from [status], which is the semantic connected/available/error vocabulary
 * rather than a brand colour chosen for the mood of the screen.
 *
 * Announced as one sentence. "Connected to fedora" and "Desktop · Linux" read
 * as two unrelated objects when a screen reader takes them separately.
 */
@Composable
fun ExchangePeerStatus(
    status: OmniBridgeStatus,
    label: String,
    identity: String,
    modifier: Modifier = Modifier,
) {
    val colors = OmniBridgeTheme.colors
    val accent = status.color()
    val tint = accent.copy(alpha = if (colors.isDark) 0.16f else 0.08f)
    Row(
        modifier = modifier
            .clip(RoundedCornerShape(OmniBridgeRadius.large))
            .background(tint)
            .padding(horizontal = OmniBridgeSpacing.md, vertical = OmniBridgeSpacing.sm)
            .semantics(mergeDescendants = true) {
                contentDescription = "$label. $identity"
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // The dot is the brand hue at full strength: a filled shape whose
        // colour is decoration, because the words beside it carry the state.
        Box(
            Modifier
                .size(10.dp)
                .clip(CircleShape)
                .background(status.dot()),
        )
        Spacer(Modifier.width(OmniBridgeSpacing.sm))
        Column {
            Text(label, style = OmniBridgeType.subtitle, color = colors.textPrimary)
            Text(identity, style = OmniBridgeType.mono, color = colors.textSecondary)
        }
    }
}

/**
 * Exactly what is about to travel.
 *
 * A titled card with room to breathe — the payload is the thing the person is
 * checking before they commit, so it gets the largest surface on the screen
 * and the most generous padding.
 *
 * @param trailing the small right-hand fact: a byte count, a file size.
 */
@Composable
fun ExchangePayloadCard(
    title: String,
    modifier: Modifier = Modifier,
    trailing: String? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = OmniBridgeTheme.colors
    OmniBridgeCard(modifier = modifier, contentPadding = OmniBridgeSpacing.lg) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(title, style = OmniBridgeType.label, color = colors.textSecondary)
            if (trailing != null) {
                Text(trailing, style = OmniBridgeType.caption, color = colors.textMuted)
            }
        }
        content()
    }
}

/**
 * An empty payload: a semantic glyph, what is true, and what to do about it.
 *
 * The glyph names the payload — a clipboard for text, a folder for files —
 * rather than decorating the space. An empty state drawn with an abstract
 * flourish tells a person nothing about which of the app's several empty
 * states they are looking at.
 */
@Composable
fun ExchangePayloadEmpty(
    @DrawableRes icon: Int,
    title: String,
    subtitle: String,
    modifier: Modifier = Modifier,
    accent: Color? = null,
) {
    val colors = OmniBridgeTheme.colors
    val hue = accent ?: colors.accentBlue
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = OmniBridgeSpacing.xl),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xs),
    ) {
        // The same mark the Files list's own empty state uses. One way of
        // saying "there is nothing here", not one per screen.
        OmniBridgeEmptyArt(icon = icon, accent = hue)
        Spacer(Modifier.height(OmniBridgeSpacing.xxs))
        Text(
            title,
            style = OmniBridgeType.subtitle,
            color = colors.textPrimary,
            textAlign = TextAlign.Center,
        )
        Text(
            subtitle,
            style = OmniBridgeType.body,
            color = colors.textSecondary,
            textAlign = TextAlign.Center,
        )
    }
}

/**
 * What is true about the link, in four facts.
 *
 * Deliberately not "end-to-end encrypted". The link is a direct, mutually
 * authenticated TLS 1.3 session pinned to the key approved at pairing — which
 * is what these four say, in the project's own terms. Reaching for a
 * marketing phrase whose meaning does not exactly match is how a security
 * claim becomes untrue, and nothing here may be strengthened without the
 * implementation being strengthened first.
 *
 * Announced as one sentence rather than as four fragments: a screen reader
 * that reads "Direct connection, TLS 1.3, Pinned identity, Local network" as
 * four separate objects makes the reader assemble the claim themselves.
 */
@Composable
fun ExchangeSecurityFooter(modifier: Modifier = Modifier) {
    val colors = OmniBridgeTheme.colors
    Column(
        modifier
            .fillMaxWidth()
            .semantics(mergeDescendants = true) {
                contentDescription =
                    "Secure connection: direct connection, TLS 1.3, pinned identity, local network."
            },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xxs),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(
                painter = painterResource(R.drawable.ic_shield_check),
                contentDescription = null,
                tint = colors.accentCyan,
                modifier = Modifier.size(OmniBridgeIconSize.small),
            )
            Spacer(Modifier.width(OmniBridgeSpacing.xxs))
            Text(
                "Secure connection",
                style = OmniBridgeType.label,
                color = colors.textSecondary,
            )
        }
        Text(
            SECURITY_FACTS.joinToString("  ·  "),
            style = OmniBridgeType.caption,
            color = colors.textMuted,
            textAlign = TextAlign.Center,
        )
    }
}

/**
 * The four facts, exactly as the implementation supports them.
 *
 * Public so the test that guards against a claim creeping upwards can read
 * them without a device.
 */
val SECURITY_FACTS: List<String> =
    listOf("Direct connection", "TLS 1.3", "Pinned identity", "Local network")

/**
 * The ambient wash behind an exchange screen.
 *
 * Two very soft corner blooms in the brand hues and nothing else. Drawn
 * behind the content, never hit-testable, and held at an alpha where it
 * cannot move any text off its contrast ratio — the background of a screen
 * whose job is to be trusted has no business being decorative enough to
 * notice.
 *
 * `drawBehind` rather than a stack of Boxes: it is two rectangles a frame
 * with no extra layout nodes, no overdraw of the content, and nothing for a
 * screen reader to find.
 *
 * ## It belongs on a full-bleed node
 *
 * The wash is drawn across the node it is applied to, and it fades to
 * transparent by *distance*, not by reaching an edge. Put it on the capped
 * content column and the column's own boundary cuts it: a faint rectangle
 * appears down both sides of a tablet, which looks like a rendering fault
 * rather than an ambient background. So it goes on the window-width node
 * behind the column — the shell's content area for
 * [io.github.yurisismotto.omnibridge.ui.Screen.SendClipboard], and the root
 * Box of the Sharesheet activity, which has no shell to sit in.
 */
@Composable
fun Modifier.exchangeAmbient(): Modifier {
    val dark = OmniBridgeTheme.colors.isDark
    // Deliberately tiny. The reference's ambient shapes are barely visible on
    // a calibrated screen, and a saturated blob would both cheapen the page
    // and start eating into text contrast.
    val alpha = if (dark) 0.10f else 0.06f
    return this.drawBehind {
        val w = size.width
        val h = size.height
        drawRect(
            brush = Brush.radialGradient(
                colors = listOf(Brand.Violet.copy(alpha = alpha), Color.Transparent),
                center = Offset(w * 1.02f, h * 0.04f),
                radius = w * 0.85f,
            ),
        )
        drawRect(
            brush = Brush.radialGradient(
                colors = listOf(Brand.Cyan.copy(alpha = alpha), Color.Transparent),
                center = Offset(-w * 0.10f, h * 0.42f),
                radius = w * 0.75f,
            ),
        )
    }
}
