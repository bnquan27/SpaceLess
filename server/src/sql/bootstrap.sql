WITH my_channels AS (
  SELECT c.*,d.role_type AS role FROM channels c JOIN channel_members cm ON cm.channel_id=c.id
  JOIN role_definitions d ON d.role_id=cm.role_id WHERE c.is_available AND cm.user_id=$1
), visible AS (
  SELECT r.*,p.lo,p.hi FROM rooms r LEFT JOIN dm_pairs p ON p.room_id=r.id
  LEFT JOIN channel_room_access a ON a.room_id=r.id AND a.user_id=$1
  WHERE r.is_available AND ((r.kind='DM' AND $1 IN (p.lo,p.hi)) OR
    a.user_id IS NOT NULL)
), room_views AS (
  SELECT r.*, jsonb_build_object('room_id',r.id::text,'channel_id',r.channel_id::text,'category_id',r.category_id::text,
    'room_name',coalesce(r.name,'Tin nhắn riêng'),'type',r.kind,'is_private',r.private,'position',coalesce(r.position,0),'last_message_sequence',r.last_sequence::text,
    'last_read_sequence',coalesce(mr.last_sequence,0)::text,
    'unread_count',(SELECT count(*) FROM (SELECT 1 FROM messages m WHERE m.room_id=r.id AND m.sequence>coalesce(mr.last_sequence,0) AND m.sender_id<>$1 LIMIT 100) unread),
    'is_read_only',r.kind='DM' AND NOT EXISTS(SELECT 1 FROM relations f WHERE f.lo=r.lo AND f.hi=r.hi AND f.status='FRIEND'),
    'member_ids',CASE WHEN r.kind='DM' THEN jsonb_build_array(r.lo::text,r.hi::text) END) AS view
  FROM visible r LEFT JOIN message_reads mr ON mr.room_id=r.id AND mr.user_id=$1
), pair_people AS (
  SELECT f.id,f.requester_id,f.status,f.created_at,CASE WHEN f.lo=$1 THEN f.hi ELSE f.lo END AS peer
  FROM relations f WHERE $1 IN(f.lo,f.hi)
), pair_views AS (
  SELECT p.*,jsonb_build_object('user_id',u.id::text,'username',u.username,'full_name',u.full_name,
    'avatar_url',CASE WHEN d.avatar_upload_id IS NOT NULL THEN '/api/v1/images/'||d.avatar_upload_id||'/url' END) AS person
  FROM pair_people p JOIN users u ON u.id=p.peer AND u.is_available JOIN user_details d ON d.user_id=u.id
), streams AS (
  SELECT 'user:'||$1::bigint::text AS key UNION ALL SELECT 'channel:'||id FROM my_channels UNION ALL SELECT 'room:'||id FROM visible
)
SELECT jsonb_build_object(
  'channels',coalesce((SELECT jsonb_agg(jsonb_build_object('channel_id',c.id::text,'channel_name',c.name,'description',c.description,
    'creator_id',c.creator_id::text,'role',c.role,'join_mode',c.join_mode,
    'invite_code',CASE WHEN c.role IN ('CREATOR','MODERATOR') AND c.join_mode='PUBLIC_LINK' THEN c.invite_code END,
    'icon_url',CASE WHEN c.icon_upload_id IS NOT NULL THEN '/api/v1/images/'||c.icon_upload_id||'/url' END,
    'background_url',CASE WHEN c.background_upload_id IS NOT NULL THEN '/api/v1/images/'||c.background_upload_id||'/url' END,
    'members',coalesce((SELECT jsonb_agg(jsonb_build_object('user_id',m.user_id::text,'role',d.role_type) ORDER BY m.user_id)
      FROM channel_members m JOIN role_definitions d ON d.role_id=m.role_id
      JOIN users u ON u.id=m.user_id AND u.is_available WHERE m.channel_id=c.id),'[]')) ORDER BY c.id) FROM my_channels c),'[]'),
  'categories',coalesce((SELECT jsonb_agg(jsonb_build_object('category_id',g.id::text,'channel_id',g.channel_id::text,
    'category_name',g.name,'position',g.position) ORDER BY g.channel_id,g.position,g.id) FROM room_categories g JOIN my_channels c ON c.id=g.channel_id),'[]'),
  'rooms',coalesce((SELECT jsonb_agg(view ORDER BY channel_id,category_id,position,id) FROM room_views WHERE kind<>'DM'),'[]'),
  'dms',coalesce((SELECT jsonb_agg(view ORDER BY id) FROM room_views WHERE kind='DM'),'[]'),
  'friends',coalesce((SELECT jsonb_agg(person ORDER BY peer) FROM pair_views WHERE status='FRIEND'),'[]'),
  'friend_requests',coalesce((SELECT jsonb_agg(jsonb_build_object('request_id',id::text,'requester_id',requester_id::text,
    'created_at',created_at,'user',person) ORDER BY id) FROM pair_views WHERE status='PENDING'),'[]'),
  'blocks',coalesce((SELECT jsonb_agg(blocked_id::text ORDER BY blocked_id) FROM user_blocks WHERE blocker_id=$1),'[]'),
  'invitations',coalesce((SELECT jsonb_agg(jsonb_build_object('invitation_id',i.id::text,'channel_id',i.channel_id::text,
    'channel_name',c.name,'inviter_id',i.inviter_id::text,'invitee_id',i.invitee_id::text,'status',i.status,'created_at',i.created_at) ORDER BY i.id)
    FROM channel_invitations i JOIN channels c ON c.id=i.channel_id AND c.is_available WHERE $1 IN(i.inviter_id,i.invitee_id) AND i.status='PENDING'),'[]'),
  'voice_states',coalesce((SELECT jsonb_agg(jsonb_build_object('room_id',v.room_id::text,'user_id',v.user_id::text,
    'voice_session_id',v.id::text,'state',CASE WHEN v.state='ISSUED' THEN 'PENDING' ELSE v.state END,
    'screen_video_sid',v.screen_video_sid,'screen_audio_sid',v.screen_audio_sid) ORDER BY v.id)
    FROM voice_sessions v JOIN visible r ON r.id=v.room_id
    WHERE v.state='CONNECTED' OR (v.state='ISSUED' AND v.join_token_expires_at>now())),'[]'),
  'revisions',coalesce((SELECT jsonb_object_agg(s.key,coalesce(v.revision,0)::text) FROM streams s LEFT JOIN stream_revisions v ON v.stream=s.key),'{}'))