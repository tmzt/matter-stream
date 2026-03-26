// Mobile home screen — Pixel 10 Pro portrait layout (412 x 915 dp)
// Status bar with centered punch-hole camera cutout (48dp)
// Ribbon card carousel in the middle
// Prompt bar fixed at bottom with padding

const [micOn, setMicOn] = useMicState();

// ── Status bar with Pixel punch-hole camera ──────────────────────────
const StatusBar = () => (
  <>
    {/* Status bar background */}
    <Slab x={0} y={0} w={412} h={48} radius={0} color="#0A0A12FF" />

    {/* Left cluster: time */}
    <Text x={16} y={14} size={14} label="9:41" color="#FFFFFFFF" />

    {/* Center: punch-hole camera cutout */}
    <Circle x={206} y={16} r={10} color="#000000FF" />
    <Circle x={206} y={16} r={8} color="#111118FF" />

    {/* Right cluster: icons + battery */}
    <Text x={310} y={14} size={11} label="5G" color="#AAAAAAFF" />
    {/* Signal bars (4 tiny slabs) */}
    <Slab x={338} y={20} w={3} h={8} radius={1} color="#AAAAAAFF" />
    <Slab x={343} y={18} w={3} h={10} radius={1} color="#AAAAAAFF" />
    <Slab x={348} y={16} w={3} h={12} radius={1} color="#AAAAAAFF" />
    <Slab x={353} y={14} w={3} h={14} radius={1} color="#AAAAAAFF" />
    {/* Battery icon */}
    <Slab x={364} y={16} w={22} h={12} radius={3} color="#44CC44FF" />
    <Slab x={386} y={19} w={3} h={6} radius={1} color="#44CC44FF" />
    <Text x={367} y={16} size={10} label="87" color="#000000FF" />
  </>
);

// ── Inbox card content ────────────────────────────────────────────────
const InboxCard = () => (
  <>
    <Slab x={10} y={10} w={392} h={740} radius={24} color="#1A1A28FF" />

    {/* Header */}
    <Text x={28} y={30} size={22} label="Inbox" color="#FFFFFFFF" />
    <Circle x={362} y={36} r={12} color="#FF4444FF" />
    <Text x={357} y={30} size={13} label="3" color="#FFFFFFFF" />

    {/* Messages */}
    <Slab x={22} y={70} w={368} h={64} radius={12} color="#222236FF" />
    <Circle x={14} y={102} r={4} color="#4488FFFF" />
    <Text x={36} y={78} size={14} label="Team standup" color="#EEEEEEFF" />
    <Text x={36} y={98} size={11} label="Notes from today's meeting" color="#777788FF" />
    <Text x={330} y={78} size={10} label="2m" color="#555566FF" />

    <Slab x={22} y={142} w={368} h={64} radius={12} color="#222236FF" />
    <Circle x={14} y={174} r={4} color="#4488FFFF" />
    <Text x={36} y={150} size={14} label="Deploy v2.4" color="#EEEEEEFF" />
    <Text x={36} y={170} size={11} label="Staging passed all checks" color="#777788FF" />
    <Text x={330} y={150} size={10} label="1h" color="#555566FF" />

    <Slab x={22} y={214} w={368} h={64} radius={12} color="#222236FF" />
    <Circle x={14} y={246} r={4} color="#4488FFFF" />
    <Text x={36} y={222} size={14} label="Invoice #1042" color="#EEEEEEFF" />
    <Text x={36} y={242} size={11} label="Payment received" color="#777788FF" />
    <Text x={330} y={222} size={10} label="3h" color="#555566FF" />

    <Slab x={22} y={286} w={368} h={64} radius={12} color="#1E1E30FF" />
    <Text x={36} y={294} size={14} label="Weekly report" color="#AAAABBFF" />
    <Text x={36} y={314} size={11} label="Metrics for the past week" color="#666677FF" />
    <Text x={320} y={294} size={10} label="1d" color="#444455FF" />

    <Slab x={22} y={358} w={368} h={64} radius={12} color="#1E1E30FF" />
    <Text x={36} y={366} size={14} label="Welcome aboard" color="#AAAABBFF" />
    <Text x={36} y={386} size={11} label="We're excited to have you" color="#666677FF" />
    <Text x={320} y={366} size={10} label="2d" color="#444455FF" />

    {/* Divider + footer */}
    <Slab x={22} y={440} w={368} h={1} radius={0} color="#2A2A3CFF" />
    <Text x={130} y={456} size={12} label="5 messages, 3 unread" color="#555566FF" />
  </>
);

// ── Calendar card content ─────────────────────────────────────────────
const CalendarCard = () => (
  <>
    <Slab x={422} y={10} w={392} h={740} radius={24} color="#1A2818FF" />

    {/* Header */}
    <Text x={440} y={30} size={22} label="Calendar" color="#FFFFFFFF" />
    <Text x={440} y={58} size={12} label="Wednesday, Mar 26" color="#88AA88FF" />

    {/* Events with color accent bars */}
    <Slab x={434} y={84} w={368} h={60} radius={12} color="#223820FF" />
    <Slab x={434} y={84} w={4} h={60} radius={2} color="#44AA44FF" />
    <Text x={450} y={94} size={14} label="Standup" color="#EEEEEEFF" />
    <Text x={450} y={114} size={11} label="9:00 - 9:15" color="#88AA88FF" />

    <Slab x={434} y={152} w={368} h={60} radius={12} color="#223820FF" />
    <Slab x={434} y={152} w={4} h={60} radius={2} color="#4488FFFF" />
    <Text x={450} y={162} size={14} label="Design review" color="#EEEEEEFF" />
    <Text x={450} y={182} size={11} label="11:00 - 12:00" color="#88AA88FF" />

    <Slab x={434} y={220} w={368} h={60} radius={12} color="#223820FF" />
    <Slab x={434} y={220} w={4} h={60} radius={2} color="#FF8844FF" />
    <Text x={450} y={230} size={14} label="1:1 with Alex" color="#EEEEEEFF" />
    <Text x={450} y={250} size={11} label="14:00 - 14:30" color="#88AA88FF" />

    <Slab x={434} y={288} w={368} h={60} radius={12} color="#1E3018FF" />
    <Text x={450} y={298} size={14} label="Sprint planning" color="#AAAABBFF" />
    <Text x={450} y={318} size={11} label="16:00 - 17:00" color="#779977FF" />

    {/* Divider + footer */}
    <Slab x={434} y={366} w={368} h={1} radius={0} color="#2A3C28FF" />
    <Text x={550} y={382} size={12} label="4 events today" color="#668866FF" />
  </>
);

// ── Settings card content ─────────────────────────────────────────────
const SettingsCard = () => (
  <>
    <Slab x={824} y={10} w={392} h={740} radius={24} color="#281A28FF" />

    {/* Header */}
    <Text x={842} y={30} size={22} label="Settings" color="#FFFFFFFF" />

    {/* Setting rows */}
    <Slab x={836} y={70} w={368} h={48} radius={12} color="#362836FF" />
    <Text x={852} y={82} size={14} label="Account" color="#EEEEEEFF" />
    <Text x={1150} y={82} size={12} label=">" color="#666677FF" />

    <Slab x={836} y={126} w={368} h={48} radius={12} color="#362836FF" />
    <Text x={852} y={138} size={14} label="Appearance" color="#EEEEEEFF" />
    <Text x={1130} y={138} size={12} label="Dark" color="#AA88AAFF" />

    <Slab x={836} y={182} w={368} h={48} radius={12} color="#362836FF" />
    <Text x={852} y={194} size={14} label="Notifications" color="#EEEEEEFF" />
    <Text x={1150} y={194} size={12} label="5" color="#FF6688FF" />

    <Slab x={836} y={238} w={368} h={48} radius={12} color="#362836FF" />
    <Text x={852} y={250} size={14} label="Privacy" color="#EEEEEEFF" />
    <Text x={1150} y={250} size={12} label=">" color="#666677FF" />

    <Slab x={836} y={294} w={368} h={48} radius={12} color="#362836FF" />
    <Text x={852} y={306} size={14} label="Storage" color="#EEEEEEFF" />
    <Text x={1110} y={306} size={12} label="2.4 GB" color="#AA88AAFF" />

    {/* Version footer */}
    <Text x={980} y={370} size={11} label="v2.1.0" color="#444455FF" />
  </>
);

// ── Prompt bar (bottom, with side padding) ────────────────────────────
const PromptBar = () => (
  <>
    <Slab x={12} y={848} w={388} h={52} radius={26} color="#1E1E2EFF" />

    {/* Mic button */}
    <Slab x={16} y={850} w={40} h={48} radius={24} color="#2A2A3CFF" />
    <Circle x={36} y={874} r={7} color="#FF4444FF" />

    {/* Input field — wider on Pixel */}
    <Slab x={60} y={850} w={216} h={48} radius={24} color="#2A2A3CFF" />
    <Text x={76} y={866} size={14} label="Ask anything..." color="#555566FF" />

    {/* Action buttons */}
    <Slab x={280} y={850} w={36} h={48} radius={18} color="#2A2A3CFF" />
    <Text x={293} y={866} size={14} label="X" color="#FF6666FF" />

    <Slab x={320} y={850} w={36} h={48} radius={18} color="#2A2A3CFF" />
    <Text x={333} y={866} size={14} label="^" color="#AAAABBFF" />

    <Slab x={360} y={850} w={36} h={48} radius={18} color="#4466FFFF" />
    <Text x={373} y={866} size={14} label=">" color="#FFFFFFFF" />
  </>
);

// ── Navigation gesture bar ───────────────────────────────────────────
const GestureBar = () => (
  <>
    <Slab x={151} y={906} w={110} h={5} radius={3} color="#444455FF" />
  </>
);

// ── Layout ────────────────────────────────────────────────────────────

{/* Background */}
<Slab x={0} y={0} w={412} h={915} radius={0} color="#0E0E16FF" />

{/* Status bar with camera cutout */}
<StatusBar />

{/* Card ribbon — below status bar, above prompt bar */}
<RibbonView x={0} y={48} w={412} h={792} scrollBank={0} cardWidth={412}>
  <InboxCard />
  <CalendarCard />
  <SettingsCard />
</RibbonView>

{/* Prompt bar with side padding */}
<PromptBar />

{/* Navigation gesture bar */}
<GestureBar />
