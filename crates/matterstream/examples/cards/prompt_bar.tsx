<>
  {/* Input bar background */}
  <Slab x={0} y={0} w={1000} h={56} radius={28} color="#1E1E2EFF" />

  {/* Input tap area */}
  <Slab x={48} y={4} w={790} h={48} radius={24} color="#2A2A3CFF" action="prompt_input" />

  {/* Placeholder text (hidden when typing) */}
  <Text x={60} y={18} size={16} label="Ask anything..." color="#555566FF" />

  {/* Pulsing voice indicator */}
  <Circle x={28} y={28} r={10} color="#5566CCFF" />
  <Circle x={28} y={28} r={5} color="#7788EEFF" />

  {/* Mic button — invisible slab for action, text label on top */}
  <Slab x={4} y={4} w={44} h={48} radius={24} color="#2A2A3CFF" action="toggle_listening" />
  <Text x={18} y={18} size={16} label="M" color="#7788EEFF" />

  {/* Clear button */}
  <Slab x={840} y={4} w={40} h={48} radius={20} color="#2A2A3CFF" action="prompt_clear" />
  <Text x={853} y={18} size={16} label="X" color="#FF6666FF" />

  {/* Expand/collapse button */}
  <Slab x={884} y={4} w={40} h={48} radius={20} color="#2A2A3CFF" action="prompt_expand" />
  <Text x={897} y={18} size={16} label="^" color="#AAAABBFF" />

  {/* Send button */}
  <Slab x={928} y={4} w={68} h={48} radius={24} color="#4466FFFF" action="prompt_send" />
  <Text x={940} y={18} size={16} label="Send" color="#FFFFFFFF" />
</>
