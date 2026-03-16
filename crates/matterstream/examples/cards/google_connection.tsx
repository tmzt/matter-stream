<>
  {/* Card background */}
  <Slab x={0} y={0} w={360} h={280} radius={12} color="#1E1E2EFF" />

  {/* Title */}
  <Text x={50} y={30} size={22} label="Google Workspace" color="#EEEEEEFF" />

  {/* Status indicator */}
  <Circle x={180} y={110} r={24} color="#33333CFF" />
  <Text x={120} y={150} size={12} label="Not connected" color="#FF6666FF" />

  {/* Connect button */}
  <Slab x={20} y={185} w={320} h={42} radius={8} color="#4285F4FF" action="google_oauth_connect" />
  <Text x={72} y={197} size={14} label="Connect with Google" color="#FFFFFFFF" />

  {/* Scope info */}
  <Text x={30} y={248} size={10} label="Scopes: mail, contacts, calendar" color="#666666FF" />
</>
