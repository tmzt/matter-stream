<>
  {/* Card background */}
  <Slab x={0} y={0} w={360} h={340} radius={12} color="#1E1E2EFF" />

  {/* Title */}
  <Text x={120} y={30} size={28} label="Sign In" color="#EEEEEEFF" />

  {/* Passkey icon area */}
  <Circle x={180} y={120} r={32} color="#282840FF" />
  <Slab x={164} y={108} w={14} h={24} radius={6} color="#6C8CFFFF" />
  <Circle x={180} y={108} r={7} color="#6C8CFFFF" />
  <Slab x={176} y={126} w={8} h={12} radius={2} color="#6C8CFFFF" />

  {/* Passkey button */}
  <Slab x={20} y={180} w={320} h={48} radius={8} color="#4466FFFF" action="passkey_login" />
  <Text x={82} y={195} size={16} label="Sign in with Passkey" color="#FFFFFFFF" />

  {/* Divider */}
  <Slab x={20} y={254} w={140} h={1} radius={0} color="#333344FF" />
  <Text x={170} y={248} size={12} label="or" color="#666666FF" />
  <Slab x={200} y={254} w={140} h={1} radius={0} color="#333344FF" />

  {/* Register passkey link */}
  <Text x={82} y={286} size={12} label="Register a new passkey" color="#5599FFFF" />
</>
