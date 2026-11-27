<div align="center">

<a href="https://lattice-writer.github.io/lattice/">
  <img src="./src-tauri/icons/app-icon.svg" alt="Lattice" width="80" />
</a>

<h1>Lattice</h1>

macOS için yerel öncelikli bir LaTeX çalışma alanı; makaleleriniz, ortak yazarlarınız ve bir yapay zeka ajanı aynı pencerede

[İndir](https://github.com/lattice-writer/lattice/releases/latest)&emsp;[Tarayıcıda deneyin](https://lattice-writer.github.io/lattice/demo/)&emsp;[Web sitesi](https://lattice-writer.github.io/lattice/)&emsp;[Filmi izleyin](https://lattice-writer.github.io/lattice/#film)&emsp;[Sürüm notları](https://github.com/lattice-writer/lattice/releases)&emsp;[Geliştirici belgeleri](docs/README.md)&emsp;[Sorunlar](https://github.com/lattice-writer/lattice/issues)&emsp;[English](README.md)&emsp;[中文](README.zh-CN.md)

[![Release](https://img.shields.io/github/v/release/lattice-writer/lattice?style=flat-square&label=release&color=556324)](https://github.com/lattice-writer/lattice/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/lattice-writer/lattice/ci.yml?branch=main&style=flat-square&label=checks)](https://github.com/lattice-writer/lattice/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-556324?style=flat-square)](LICENSE)

</div>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/hero-dark.webp" />
    <img src="docs/images/hero-light.webp" alt="Bir LaTeX makalesi açıkken Lattice: solda Proje ağacı ve Makaleler kütüphanesi, ortada kaynak kod, sağda derlenmiş PDF" width="900" />
  </picture>
</p>

## Neden Lattice

Bir makale tek bir `.tex` dosyasından ibaret değildir: kontrol ettiğiniz PDF, atıf yaptığınız makaleler, ortak yazarlarınızın bıraktığı yorumlar ve artık sizinle birlikte düzenleme yapan bir ajandır.
Lattice bunların hepsini tek bir yerel Mac penceresinde bir arada tutar.
Projeniz diskte sıradan bir klasör olarak kalır; böylece çevrimdışı çalışır, başka herhangi bir düzenleyicide açılır ve Git ile uyumludur; Overleaf makalenizin yaşadığı yer değil, yalnızca bir eşitleme hedefidir.

## Yazın

Yazarken tamamlama ve tanılama, çift yönlü SyncTeX atlamaları ve kendi TeX Live kurulumunuzla tek tıkla derleme ile kaynak kod ve PDF yan yana (veya ikinci bir ekran için PDF ayrı bir pencerede).
Bir `.tex` sekmesini **Source** görünümünden **Visual** görünümüne geçirerek başlıkları, vurguları, denklemleri, listeleri ve şekilleri aynı düzenleyici içinde yerinde çizilmiş hâliyle okuyup düzenleyin.
Bir anahat şeridi bölümlerinizi takip eder, `\cite` anahtarları üzerine gelindiğinde referansını gösterir ve anlamları tanımlı altı vurgulayıcı hâlâ üzerinde çalışılması gereken yerleri işaretler.
Bir NeurIPS, ICML, ICLR, ACL, CVPR ya da AAAI şablonundan veya iki tez sınıfından biriyle başlayın; **Export for arXiv** kendi başına derlenen tek bir ZIP oluşturur.
Herhangi bir paneli istediğiniz yere sabitleyin, Yazma, Okuma ve Kaynakça düzenlerini saklayın veya kendi çalışma alanlarınızı kaydedin.

## Gözden Geçirin

<img src="docs/images/review.webp" alt="2. turda üç öneri içeren Açıklamalar paneli; ajanın özetin açılış cümlesini yeniden yazımı main.tex içinde Kabul Et, Reddet ve Yeniden Dene seçenekleriyle satır içi açık" width="900" />

Her düzenleme için bir gerekçeyle birlikte yerinde **Proofread** (Yazım Denetimi) veya **Polish** (İyileştirme) yapmak üzere metin seçin.
Kaynak koda veya PDF'e notlar ekleyin, ardından notlarınızdan turlar hâlinde düzeltme yapın: ajan her biri için bir düzenleme önerir ve siz bunu yerinde kabul eder veya reddedersiniz.
Notlar ve Overleaf yorumları PDF'in yanındaki bir sütunda da hizalanır; her seviye sayfadaki işaretiyle gösterilir ve bunları orada okuyabilir, yanıtlayabilir ve çözümleyebilirsiniz.
**Export review PDF**, işaretlediğiniz notları derlenmiş PDF'in bir kopyasına standart PDF yorumları olarak yazar; böylece iş birliği yaptığınız kişiler bunları Preview, Acrobat veya Zotero'da okuyabilir.

### Kendi ajanınızı getirin

Agent paneli [Claude Code](https://github.com/anthropics/claude-code) veya [Codex](https://github.com/openai/codex) ya da Mac'inizde kendi hesabınızla oturum açılmış Cursor, Antigravity (`agy`), OpenCode veya Droid gibi başka bir ajan CLI'ını çalıştırır.
Ya da Settings › Agent › Model endpoint bölümünden kendi OpenAI veya Anthropic uyumlu API'nizi bağlayın: bir adres, (Anahtar Zincirinizde saklanan) bir anahtar ve model adları; ajan bunu herhangi bir komut satırı aracı kurmaya gerek kalmadan Pi üzerinden çalıştırır.
Lattice kendi içinde bir model veya yapay zeka aboneliğiyle gelmez.
Ajan projenizi ve makalelerinizi okur, derleme hatalarını düzeltir ve dosyaları düzenler; her değişiklik inceleyebileceğiniz veya geri alabileceğiniz bir diff olarak gösterilir.
Ayrıca Zotero veya Heptabase gibi bu CLI'a eklediğiniz MCP sunucularını da kullanabilir: bkz. [Zotero, Heptabase ve Agent paneli](docs/zotero-and-notes.md).

## Araştırın

<img src="docs/images/papers.webp" alt="Solda dokuz makale bulunan Makaleler kütüphanesi, ortada okunabilir bir sayfa olarak açık Mamba: Linear-Time Sequence Modeling with Selective State Spaces makalesi ve sağda derlenmiş taslak" width="900" />

<sub>Gösterilen makale: Albert Gu ve Tri Dao, “Mamba: Linear-Time Sequence Modeling with Selective State Spaces”, arXiv:2312.00752, CC BY 4.0.</sub>

arXiv kimliği, DOI, başlık, bağlantı veya BibTeX ile ya da doğrudan Zotero kütüphanenizden makale ekleyin; her birini taslağınızın yanında bir blog yazısı, makale veya PDF olarak okuyun.
Üzerine gelindiğinde referansı görerek tek adımda atıf yapın.
Kütüphane atıf yaptıklarınıza göre sıralar, filtreler ve gruplar; **Check references** kaynakçanızı Crossref üzerinden denetler ve her bulguyu bir klavye kuyruğunda gösterir.

## Overleaf ile İş Birliği Yapın

<img src="docs/images/collab.webp" alt="Overleaf yorum vurguları ve bir ortak yazarın canlı imleciyle main.tex; yanında demo ortak yazarlardan üç konu içeren Overleaf panelinin Yorumlar sekmesi ile Öneriler ve Sohbet sekmeleri" width="900" />

Canlı varlık bilgisi, sohbet, yorumlar ve izlenen değişikliklerle çift yönlü Overleaf eşitlemesi.
Ortak yazarlarınız Overleaf'te kalır ve düzenlemeleri yazdıkları anda gelir; başka kimsenin Lattice kullanması gerekmez.
Yorumlar kişi başına tek bir renkle konuşma düzeninde okunur ve kenar boşluğu incelemesi yorumlar ile önerileri ilgili satırların yanına yerleştirir.

## Her Format

<img src="docs/images/formats.webp" alt="Biçimlendirilmiş bir Markdown belgesi olarak gösterilen notes.md'nin yanında düzenleyicide açık attention-map.tldr beyaz tahtası; Proje ağacı aynı projenin HTML sayfasını, elektronik tablosunu ve slaytlarını listeler" width="900" />

Bloklar hâlinde düzenlenen Markdown notları, Excalidraw beyaz tahtaları, etkileşimli HTML sayfaları ve formüllü elektronik tablolar makaleyle aynı projede yer alır.
Slaytlar [Open Slide](https://github.com/open-slide/open-slide) altyapısını kullanan React sunumlarıdır: bunları düzenleyin, üzerlerine yorum yapın, ajanın revize etmesini sağlayın ve Lattice içinden sunum yapın.
**Import files…** Mac'inizdeki diğer her şeyi içeri aktarır.

## Geçmiş

<img src="docs/images/history.webp" alt="Geçmiş paneli: ajanın özeti yeniden yazımı Bu dosyayı geri yükle seçeneğiyle diff'ine genişletilmiş; ardından bir atıf, kendi düzenlemeniz ve ortak yazarların kaydettiği sürümler, sağda ise derlenmiş PDF" width="900" />

Kaydedilen sürümler, sizin veya ajanın yaptığı her düzenleme ve Overleaf geçmişi için tek bir zaman çizelgesi.
Herhangi bir kaydı açarak diff'ini görün ve bir dosyayı geri yükleyin ya da herhangi iki sürümü **Compare…** ile karşılaştırıp değişikliklerin işaretlendiği PDF'i (TeX Live'ınızdaki `latexdiff` aracılığıyla) dışa aktarın.
**Track changes** inceleme modunu açar: o andan itibaren yaptığınız düzenlemeler siz yazarken kaynak kodda işaretlenir, PDF de bunları işaretli olarak gösterebilir ve bir Overleaf planına gerek kalmadan her değişikliği tek tek veya tümünü birden kabul edip reddedebilirsiniz.
Proje bir depo olduğunda Git paneli değişiklikleri ve pull request'leri gösterir.

## Kurulum

Lattice, Apple silicon veya Intel işlemcili macOS 14 ve üzeri Mac'lerde çalışır ve kendi [MacTeX](https://tug.org/mactex/) veya TeX Live kurulumunuzla derler.
Henüz TeX kurulu değil mi? Lattice sizin için BasicTeX kurmayı teklif eder: bu Başlangıç kontrol listesinin ilk adımıdır ve TeX bulamayan bir derleme de bunu önerir.

1. [Son sürümden](https://github.com/lattice-writer/lattice/releases/latest) `.dmg` dosyasını indirin ve Lattice'i Applications klasörüne sürükleyin.
   Intel işlemcili bir Mac'te `_x64.dmg` ile biten dosyayı indirin (Apple menüsü → Bu Mac Hakkında penceresi Apple silicon'da **Çip**, Intel'de **İşlemci** gösterir).
   Veya Homebrew ile kurun: `brew install --cask lattice`. Uygulama kendini günceller, bu nedenle Homebrew yükseltmeleri uygulamaya bırakır.
2. Bir klasör açın, bir şablondan başlayın veya bir Overleaf projesi bağlayın.
   Proje menüsündeki **Guided tutorial**, Başlangıç listesinin (PDF derlemekten ajanın bir paragrafı revize etmesine izin vermeye kadar altı adımlı kontrol listesi) yanında örnek bir makale açar.

Resmi derlemeler Apple tarafından imzalanır, noter onayından geçer ve kendilerini günceller.

**Linux (deneysel).** Linux kontrollerinden geçen sürümler, Ubuntu 22.04 veya üzeri, Fedora 45 ve benzeri dağıtımlar için x86_64 `.AppImage` dosyası da içerir.
Çalıştırılabilir yapın (`chmod +x Lattice_*.AppImage`) ve çalıştırın; dağıtımınızın paketlerindeki TeX Live ile derler.
Linux'ta Lattice yeni sürümleri kontrol eder ve GitHub Releases'a bağlantı verir ancak kendini güncellemez; her yeni sürümü kendiniz indirmelisiniz. Yerleşik Ajan, dağıtımınızdan `bubblewrap` ve FUSE (`fuse3`), bubblewrap'in korumalı alan başlatmasına izin veren bir sistem ve Linux 6.12 veya üzeri gerektirir; bunlardan biri eksikse Ajan kapalı kalır ve paneli hangisinin eksik olduğunu bildirir. Daha eski bir çekirdekte bir ayar, azaltılmış yalıtımla yine de çalıştırır (Settings › Agent › Environment).

Windows (deneysel). Windows kontrollerinden geçen sürümler, SHA-256 sağlama toplamı ve kurulum talimatlarıyla birlikte imzasız x64 `windows-x64-experimental-setup.exe` dosyasını da içerir. SmartScreen bilinmeyen yayıncı hakkında uyarı verebilir. Ajan ve otomatik güncellemeler kullanılamaz; Lattice kurulum iletişim kutusundan sizin için TeX Live kurabilir veya kendiniz kurabilirsiniz. Gereksinimler ve sınırlamalar için [Windows kılavuzuna](docs/windows-host.md) bakın.

## Katkıda Bulunma

Kaynaktan derlemek için [CONTRIBUTING.md](CONTRIBUTING.md) ve [mimari belgelerine](docs/README.md) bakın.
Hata bildirimleri ve pull request'ler memnuniyetle karşılanır; büyük bir yeniden düzenlemeden önce lütfen bir [issue](https://github.com/lattice-writer/lattice/issues) açın.

## Teşekkürler

Lattice birçok açık kaynak projenin üzerinde yükselmektedir. Başlıcaları:

- [Open Knowledge](https://github.com/inkeep/open-knowledge) (inkeep/open-knowledge) — Inkeep tarafından geliştirilen ve Lattice'in önceki görsel Markdown düzenleyicisinin özgün temelini oluşturan proje. Lattice o zamandan beri kendi motoruna geçti ancak düzenleyici oradan başladı ve kendilerine minnettarız.
- [Trellis](https://github.com/DanFessler/trellis) — Dan Fessler tarafından geliştirilen, Lattice çalışma alanının sabitlenebilir panel düzeni. DanFessler tarafından geliştirilen Trellis kullanılmaktadır - github.com/DanFessler/trellis.
- [Synara](https://github.com/Emanuele-web04/synara) — T3 Tools Inc. ve Emanuele Di Pietro tarafından geliştirilen, Lattice'in yapay zeka asistanının arkasındaki ajan çalışma zamanı.
- [Open Slide](https://github.com/open-slide/open-slide) — sunumlara güç veren proje.
- [Tiptap](https://github.com/ueberdosis/tiptap) ve [CodeMirror](https://codemirror.net/) — zengin metin ve kaynak kod düzenleyicileri.
- [PDF.js](https://github.com/mozilla/pdf.js) — PDF görüntüleyici.
- [Excalidraw](https://github.com/excalidraw/excalidraw) — beyaz tahta.
- [Tauri](https://github.com/tauri-apps/tauri) — masaüstü kabuğu.

## Lisans

Lattice, T3 Tools Inc. ve Emanuele Di Pietro tarafından geliştirilen MIT lisanslı [Synara](https://github.com/Emanuele-web04/synara) ajan çalışma zamanıyla birlikte [Apache License 2.0](LICENSE) altında lisanslanmıştır (ayrıca bkz. [NOTICE](NOTICE)).
0.1.341 dahil ve öncesi sürümler GPL-3.0-or-later lisanslıydı ve bu koşullar altında kullanılabilir olmaya devam etmektedir.
Bileşen lisansları için [üçüncü taraf bildirimlerine](THIRD_PARTY_NOTICES.md) ve çalışma zamanı ayrıntıları için [Synara entegrasyon notlarına](docs/synara-runtime.md) bakın.

[`gpui/`](gpui/) içindeki GPUI önizleme uygulaması, Zed'in GPL-3.0-or-later düzenleyici crate'lerini bağladığı için GPL-3.0-or-later ([`gpui/LICENSE`](gpui/LICENSE)) altında ayrı bir programdır; [kendi README'sinde](gpui/README.md#license) listelendiği gibi birkaç parçası Apache-2.0 lisanslıdır.
`gpui/` dışındaki her şey Apache-2.0 olarak kalır.

Bir bileşenin kendi koşulları vardır: çalışma alanının panel düzeni `@danfessler/trellis` (`@danfessler/trellis-react` ile birlikte), ticari bir katmana sahip kendi ticari olmayan lisansı altındadır; bkz. [lisansı](public/licenses/trellis-LICENSE.md), [ticari olmayan koşullar](public/licenses/trellis-LICENSE-NONCOMMERCIAL.md) ve [ticari koşullar](public/licenses/trellis-LICENSE-COMMERCIAL.md). Bu koşullar Lattice'i kullanan herkes için geçerlidir: ücretsiz katman yalnızca ticari olmayan kullanımı kapsar; dolayısıyla bir şirkette veya kâr amacı güden bir kuruluşta kullanım için yazarından bir Trellis lisansı alınması gerekir. DanFessler tarafından geliştirilen Trellis kullanılmaktadır - github.com/DanFessler/trellis.
