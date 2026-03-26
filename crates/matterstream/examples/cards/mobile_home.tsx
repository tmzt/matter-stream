// Mobile home screen — portrait layout (390 x 844)
// Status bar area at top (54px cutout)
// Ribbon card carousel in the middle
// Prompt bar fixed at bottom with padding

const [micOn, setMicOn] = useMicState();

// ── Status bar (top, below cutout) ────────────────────────────────────
const StatusBar = () => (
  <>
    <Slab x={0} y={0} w={390} h={54} radius={0} color="#0A0A12FF" />
    <Text x={16} y={18} size={14} label="9:41" color="#FFFFFFFF" />
    <Text x={330} y={18} size={12} label="100%" color="#AAAAAAFF" />
    <Circle x={318} y={24} r={5} color="#44CC44FF" />
  </>
);

// ── Inbox card content ────────────────────────────────────────────────
const InboxCard = () => (
  <>
    <Slab x={10} y={10} w={370} h={680} radius={20} color="#1A1A28FF" />

    {/* Header */}
    <Text x={30} y={30} size={22} label="Inbox" color="#FFFFFFFF" />
    <Circle x={340} y={36} r={12} color="#FF4444FF" />
    <Text x={335} y={30} size={13} label="3" color="#FFFFFFFF" />

    {/* Messages */}
    <Slab x={24} y={70} w={342} h={64} radius={12} color="#222236FF" />
    <Circle x={16} y={102} r={4} color="#4488FFFF" />
    <Text x={38} y={78} size={14} label="Team standup" color="#EEEEEEFF" />
    <Text x={38} y={98} size={11} label="Notes from today's meeting" color="#777788FF" />
    <Text x={310} y={78} size={10} label="2m" color="#555566FF" />

    <Slab x={24} y={142} w={342} h={64} radius={12} color="#222236FF" />
    <Circle x={16} y={174} r={4} color="#4488FFFF" />
    <Text x={38} y={150} size={14} label="Deploy v2.4" color="#EEEEEEFF" />
    <Text x={38} y={170} size={11} label="Staging passed all checks" color="#777788FF" />
    <Text x={310} y={150} size={10} label="1h" color="#555566FF" />

    <Slab x={24} y={214} w={342} h={64} radius={12} color="#222236FF" />
    <Circle x={16} y={246} r={4} color="#4488FFFF" />
    <Text x={38} y={222} size={14} label="Invoice #1042" color="#EEEEEEFF" />
    <Text x={38} y={242} size={11} label="Payment received" color="#777788FF" />
    <Text x={310} y={222} size={10} label="3h" color="#555566FF" />

    <Slab x={24} y={286} w={342} h={64} radius={12} color="#1E1E30FF" />
    <Text x={38} y={294} size={14} label="Weekly report" color="#AAAABBFF" />
    <Text x={38} y={314} size={11} label="Metrics for the past week" color="#666677FF" />
    <Text x={300} y={294} size={10} label="1d" color="#444455FF" />

    <Slab x={24} y={358} w={342} h={64} radius={12} color="#1E1E30FF" />
    <Text x={38} y={366} size={14} label="Welcome aboard" color="#AAAABBFF" />
    <Text x={38} y={386} size={11} label="We're excited to have you" color="#666677FF" />
    <Text x={300} y={366} size={10} label="2d" color="#444455FF" />

    {/* Footer */}
    <Slab x={24} y={440} w={342} h={1} radius={0} color="#2A2A3CFF" />
    <Text x={120} y={455} size={12} label="5 messages, 3 unread" color="#555566FF" />
  </>
);

// ── Calendar card content ─────────────────────────────────────────────
const CalendarCard = () => (
  <>
    <Slab x={400} y={10} w={370} h={680} radius={20} color="#1A2818FF" />

    {/* Header */}
    <Text x={420} y={30} size={22} label="Calendar" color="#FFFFFFFF" />
    <Text x={420} y={58} size={12} label="Wednesday, Mar 26" color="#88AA88FF" />

    {/* Events */}
    <Slab x={414} y={84} w={342} h={56} radius={12} color="#223820FF" />
    <Slab x={414} y={84} w={4} h={56} radius={2} color="#44AA44FF" />
    <Text x={430} y={92} size={14} label="Standup" color="#EEEEEEFF" />
    <Text x={430} y={112} size={11} label="9:00 - 9:15" color="#88AA88FF" />

    <Slab x={414} y={148} w={342} h={56} radius={12} color="#223820FF" />
    <Slab x={414} y={148} w={4} h={56} radius={2} color="#4488FFFF" />
    <Text x={430} y={156} size={14} label="Design review" color="#EEEEEEFF" />
    <Text x={430} y={176} size={11} label="11:00 - 12:00" color="#88AA88FF" />

    <Slab x={414} y={212} w={342} h={56} radius={12} color="#223820FF" />
    <Slab x={414} y={212} w={4} h={56} radius={2} color="#FF8844FF" />
    <Text x={430} y={220} size={14} label="1:1 with Alex" color="#EEEEEEFF" />
    <Text x={430} y={240} size={11} label="14:00 - 14:30" color="#88AA88FF" />

    <Slab x={414} y={276} w={342} h={56} radius={12} color="#1E3018FF" />
    <Text x={430} y={284} size={14} label="Sprint planning" color="#AAAABBFF" />
    <Text x={430} y={304} size={11} label="16:00 - 17:00" color="#779977FF" />

    {/* Footer */}
    <Slab x={414} y={350} w={342} h={1} radius={0} color="#2A3C28FF" />
    <Text x={520} y={365} size={12} label="4 events today" color="#668866FF" />
  </>
);

// ── Settings card content ─────────────────────────────────────────────
const SettingsCard = () => (
  <>
    <Slab x={790} y={10} w={370} h={680} radius={20} color="#281A28FF" />

    {/* Header */}
    <Text x={810} y={30} size={22} label="Settings" color="#FFFFFFFF" />

    {/* Sections */}
    <Slab x={804} y={70} w={342} h={48} radius={12} color="#362836FF" />
    <Text x={820} y={82} size={14} label="Account" color="#EEEEEEFF" />
    <Text x={1080} y={82} size={12} label=">" color="#666677FF" />

    <Slab x={804} y={126} w={342} h={48} radius={12} color="#362836FF" />
    <Text x={820} y={138} size={14} label="Appearance" color="#EEEEEEFF" />
    <Text x={1060} y={138} size={12} label="Dark" color="#AA88AAFF" />

    <Slab x={804} y={182} w={342} h={48} radius={12} color="#362836FF" />
    <Text x={820} y={194} size={14} label="Notifications" color="#EEEEEEFF" />
    <Text x={1080} y={194} size={12} label="5" color="#FF6688FF" />

    <Slab x={804} y={238} w={342} h={48} radius={12} color="#362836FF" />
    <Text x={820} y={250} size={14} label="Privacy" color="#EEEEEEFF" />
    <Text x={1080} y={250} size={12} label=">" color="#666677FF" />

    <Slab x={804} y={294} w={342} h={48} radius={12} color="#362836FF" />
    <Text x={820} y={306} size={14} label="Storage" color="#EEEEEEFF" />
    <Text x={1040} y={306} size={12} label="2.4 GB" color="#AA88AAFF" />

    {/* Footer */}
    <Text x={910} y={370} size={11} label="v2.1.0" color="#444455FF" />
  </>
);

// ── Prompt bar (bottom) ───────────────────────────────────────────────
const PromptBar = () => (
  <>
    <Slab x={12} y={756} w={366} h={52} radius={26} color="#1E1E2EFF" />

    {/* Mic button */}
    <Slab x={16} y={758} w={40} h={48} radius={24} color="#2A2A3CFF" />
    <Circle x={36} y={782} r={7} color="#FF4444FF" />

    {/* Input field */}
    <Slab x={60} y={758} w={198} h={48} radius={24} color="#2A2A3CFF" />
    <Text x={76} y={774} size={14} label="Ask anything..." color="#555566FF" />

    {/* Action buttons */}
    <Slab x={262} y={758} w={36} h={48} radius={18} color="#2A2A3CFF" />
    <Text x={275} y={774} size={14} label="X" color="#FF6666FF" />

    <Slab x={302} y={758} w={36} h={48} radius={18} color="#2A2A3CFF" />
    <Text x={315} y={774} size={14} label="^" color="#AAAABBFF" />

    <Slab x={342} y={758} w={32} h={48} radius={16} color="#4466FFFF" />
    <Text x={353} y={774} size={14} label=">" color="#FFFFFFFF" />
  </>
);

// ── Home indicator bar ────────────────────────────────────────────────
const HomeIndicator = () => (
  <>
    <Slab x={140} y={826} w={110} h={5} radius={3} color="#444455FF" />
  </>
);

// ── Layout ────────────────────────────────────────────────────────────

{/* Background */}
<Slab x={0} y={0} w={390} h={844} radius={0} color="#0E0E16FF" />

{/* Status bar */}
<StatusBar />

{/* Card ribbon — below status bar, above prompt bar */}
<RibbonView x={0} y={54} w={390} h={696} scrollBank={0} cardWidth={390}>
  <InboxCard />
  <CalendarCard />
  <SettingsCard />
</RibbonView>

{/* Prompt bar with padding */}
<PromptBar />

{/* Home indicator */}
<HomeIndicator />
