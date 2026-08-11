import { Award, ChevronRight, LockKeyhole, Settings, Sparkles, UserRound } from 'lucide-react-native';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import { ScreenHeader } from '../components/ScreenHeader';
import { colors } from '../theme';

const links = [
  { label: '成长回望', icon: Sparkles },
  { label: '我的成就', icon: Award },
  { label: '隐私与权限', icon: LockKeyhole },
  { label: '设置', icon: Settings },
];

export function ProfileScreen() {
  return (
    <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
      <ScreenHeader title="我的" />
      <View style={styles.identity}>
        <View style={styles.avatar}><UserRound color={colors.evergreen} size={34} /></View>
        <View style={styles.identityCopy}>
          <Text style={styles.name}>行路人</Text>
          <Text style={styles.caption}>在万卷与山河之间，成为自己</Text>
        </View>
      </View>
      <View style={styles.stats}>
        <ProfileStat label="同行天数" value="68" />
        <ProfileStat label="完成行动" value="142" />
        <ProfileStat label="留下行记" value="17" />
      </View>
      <Text style={styles.sectionTitle}>成长资产</Text>
      <View style={styles.links}>
        {links.map(({ label, icon: Icon }) => (
          <Pressable key={label} style={({ pressed }) => [styles.link, pressed && styles.pressed]}>
            <View style={styles.linkIcon}><Icon color={colors.evergreen} size={19} /></View>
            <Text style={styles.linkText}>{label}</Text>
            <ChevronRight color={colors.faint} size={18} />
          </Pressable>
        ))}
      </View>
    </ScrollView>
  );
}

function ProfileStat({ label, value }: { label: string; value: string }) {
  return (
    <View style={styles.stat}>
      <Text style={styles.statValue}>{value}</Text>
      <Text style={styles.statLabel}>{label}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 30 },
  identity: { flexDirection: 'row', alignItems: 'center', gap: 14, paddingHorizontal: 20, paddingVertical: 16 },
  avatar: { width: 64, height: 64, borderRadius: 32, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.evergreenSoft },
  identityCopy: { flex: 1, minWidth: 0 },
  name: { color: colors.ink, fontSize: 20, fontWeight: '700', letterSpacing: 0 },
  caption: { color: colors.muted, fontSize: 12, lineHeight: 18, marginTop: 4, letterSpacing: 0 },
  stats: { minHeight: 90, flexDirection: 'row', alignItems: 'center', marginTop: 10, backgroundColor: colors.ink },
  stat: { flex: 1, alignItems: 'center' },
  statValue: { color: colors.surface, fontSize: 19, fontWeight: '800', letterSpacing: 0 },
  statLabel: { color: '#BBC1BD', fontSize: 10, marginTop: 4, letterSpacing: 0 },
  sectionTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', marginHorizontal: 20, marginTop: 26, marginBottom: 10, letterSpacing: 0 },
  links: { backgroundColor: colors.surface, borderTopColor: colors.line, borderBottomColor: colors.line, borderTopWidth: StyleSheet.hairlineWidth, borderBottomWidth: StyleSheet.hairlineWidth },
  link: { minHeight: 58, marginLeft: 20, paddingRight: 20, flexDirection: 'row', alignItems: 'center', gap: 12, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth },
  pressed: { opacity: 0.55 },
  linkIcon: { width: 30, height: 30, borderRadius: 6, backgroundColor: colors.evergreenSoft, alignItems: 'center', justifyContent: 'center' },
  linkText: { flex: 1, color: colors.ink, fontSize: 14, fontWeight: '600', letterSpacing: 0 },
});

