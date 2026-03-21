const IconButton = ({ w, h, radius, color, action, label, labelColor }) => (
  <Slab w={w} h={h} radius={radius} color={color} action={action} halign="center" valign="center">
    <Text size={16} label={label} color={labelColor} />
  </Slab>
)

const VoiceIndicator = ({ x, y }) => (
  <>
    <Circle x={x} y={y} r={10} color="#5566CCFF" />
    <Circle x={x} y={y} r={5} color="#7788EEFF" />
  </>
)

<>
  <Slab x={0} y={0} w={1000} h={56} radius={28} color="#1E1E2EFF" layout="hstack" padding={4} gap={4} valign="center">

    <IconButton w={44} h={48} radius={24} color="#2A2A3CFF" action="toggle_listening" label="M" labelColor="#7788EEFF" />

    <Slab w={790} h={48} radius={24} color="#2A2A3CFF" action="prompt_input" valign="center" padding={12}>
      <Text size={16} label="Ask anything..." color="#555566FF" />
    </Slab>

    <IconButton w={40} h={48} radius={20} color="#2A2A3CFF" action="prompt_clear" label="X" labelColor="#FF6666FF" />
    <IconButton w={40} h={48} radius={20} color="#2A2A3CFF" action="prompt_expand" label="^" labelColor="#AAAABBFF" />
    <IconButton w={68} h={48} radius={24} color="#4466FFFF" action="prompt_send" label="Send" labelColor="#FFFFFFFF" />

  </Slab>

  <VoiceIndicator x={28} y={28} />
</>
