// Ribbon demo — 3 cards in a horizontally scrollable ribbon view.
// Drag left/right to scroll. Cards snap with momentum + friction physics.
// scrollBank={0} means scalar_bank[0] controls the scroll offset.

<RibbonView x={0} y={0} w={400} h={300} scrollBank={0} cardWidth={400}>
  {/* Card 1 */}
  <Slab x={10} y={10} w={380} h={280} radius={16} color="#1E1E2EFF" />
  <Text x={30} y={40} size={24} label="Inbox" color="#EEEEEEFF" />
  <Text x={30} y={80} size={14} label="3 new messages" color="#888888FF" />
  <Slab x={30} y={120} w={340} h={50} radius={8} color="#282840FF" />
  <Text x={50} y={135} size={12} label="Meeting at 2pm" color="#AAAAAAFF" />
  <Slab x={30} y={180} w={340} h={50} radius={8} color="#282840FF" />
  <Text x={50} y={195} size={12} label="Deploy v2.1 ready" color="#AAAAAAFF" />

  {/* Card 2 */}
  <Slab x={410} y={10} w={380} h={280} radius={16} color="#1E2E1EFF" />
  <Text x={430} y={40} size={24} label="Calendar" color="#EEEEEEFF" />
  <Text x={430} y={80} size={14} label="Today's schedule" color="#88AA88FF" />
  <Slab x={430} y={120} w={340} h={50} radius={8} color="#284028FF" />
  <Text x={450} y={135} size={12} label="Standup 9:00" color="#AAAAFFFF" />
  <Slab x={430} y={180} w={340} h={50} radius={8} color="#284028FF" />
  <Text x={450} y={195} size={12} label="Review 14:00" color="#AAAAFFFF" />

  {/* Card 3 */}
  <Slab x={810} y={10} w={380} h={280} radius={16} color="#2E1E2EFF" />
  <Text x={830} y={40} size={24} label="Settings" color="#EEEEEEFF" />
  <Text x={830} y={80} size={14} label="Account & Prefs" color="#AA88AAFF" />
  <Slab x={830} y={120} w={340} h={100} radius={8} color="#402840FF" />
  <Text x={850} y={140} size={12} label="Dark Mode: On" color="#CCAACCFF" />
  <Text x={850} y={170} size={12} label="Notifications: 5" color="#CCAACCFF" />
</RibbonView>
