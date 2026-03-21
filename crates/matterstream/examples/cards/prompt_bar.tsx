<>
  {/* Input bar background */}
  <Slab x={0} y={0} w={1000} h={56} radius={28} color="#1E1E2EFF" />

  {/* Inner input field */}
  <Slab x={4} y={4} w={992} h={48} radius={24} color="#2A2A3CFF" action="prompt_input" />

  {/* Placeholder text (hidden when typing) */}
  <Text x={56} y={18} size={16} label="Ask anything..." color="#555566FF" />

  {/* Pulsing voice indicator — concentric rings */}
  <Circle x={28} y={28} r={16} color="#3A3A50FF" action="toggle_listening" />
  <Circle x={28} y={28} r={10} color="#5566CCFF" />
  <Circle x={28} y={28} r={5} color="#7788EEFF" />

  {/* Clear (x) button */}
  <Slab x={892} y={12} w={32} h={32} radius={16} color="#3A3A50FF" action="prompt_clear" />
  <Text x={901} y={17} size={14} label="x" color="#FF6666FF" />

  {/* Send button */}
  <Slab x={944} y={8} w={48} h={40} radius={20} color="#4466FFFF" action="prompt_send" />
  <Text x={958} y={18} size={16} label="->" color="#FFFFFFFF" />
</>
