import { BookOpenText, Route, X } from 'lucide-react-native';
import { Modal, Pressable, StyleSheet, Text, View } from 'react-native';

import { colors } from '../theme';

type Props = {
  visible: boolean;
  onClose: () => void;
  onCreateEntry: () => void;
  onCreateJourney: () => void;
};

export function CreateMenuModal({ visible, onClose, onCreateEntry, onCreateJourney }: Props) {
  return (
    <Modal animationType="fade" onRequestClose={onClose} transparent visible={visible}>
      <View style={styles.overlay}>
        <Pressable accessibilityLabel="关闭创作菜单" onPress={onClose} style={StyleSheet.absoluteFill} />
        <View style={styles.sheet}>
          <View style={styles.header}><Text style={styles.title}>创作</Text><Pressable accessibilityLabel="关闭" hitSlop={10} onPress={onClose} style={styles.close}><X color={colors.ink} size={21} /></Pressable></View>
          <Pressable onPress={onCreateEntry} style={({ pressed }) => [styles.option, pressed && styles.pressed]}>
            <View style={[styles.optionIcon, styles.entryIcon]}><BookOpenText color={colors.evergreen} size={21} /></View>
            <View style={styles.copy}><Text style={styles.optionTitle}>记录此刻</Text><Text style={styles.optionText}>写下行动、感受、数值或地点</Text></View>
          </Pressable>
          <Pressable onPress={onCreateJourney} style={({ pressed }) => [styles.option, pressed && styles.pressed]}>
            <View style={[styles.optionIcon, styles.routeIcon]}><Route color={colors.blue} size={21} /></View>
            <View style={styles.copy}><Text style={styles.optionTitle}>创建路线</Text><Text style={styles.optionText}>从一条可执行的长期计划开始</Text></View>
          </Pressable>
        </View>
      </View>
    </Modal>
  );
}

const styles = StyleSheet.create({
  overlay: { flex: 1, justifyContent: 'flex-end', backgroundColor: 'rgba(24, 29, 26, 0.36)' },
  sheet: { paddingBottom: 28, borderTopLeftRadius: 8, borderTopRightRadius: 8, backgroundColor: colors.surface },
  header: { height: 60, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  title: { color: colors.ink, fontSize: 18, fontWeight: '700', letterSpacing: 0 },
  close: { width: 38, height: 38, alignItems: 'center', justifyContent: 'center' },
  option: { minHeight: 80, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', gap: 13, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  optionIcon: { width: 42, height: 42, alignItems: 'center', justifyContent: 'center', borderRadius: 7 },
  entryIcon: { backgroundColor: colors.evergreenSoft },
  routeIcon: { backgroundColor: colors.blueSoft },
  copy: { flex: 1, minWidth: 0 },
  optionTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  optionText: { color: colors.muted, fontSize: 12, lineHeight: 18, marginTop: 3, letterSpacing: 0 },
  pressed: { opacity: 0.62 },
});
