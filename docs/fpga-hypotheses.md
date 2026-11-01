# Hipóteses do nível 4 (FPGA)

Documento de trabalho do nível 4 do roadmap: síntese física do core ternário em FPGA
(Field-Programmable Gate Array, matriz de portas programável em campo). Cada item abaixo é uma
hipótese falsificável, com o experimento que a testa, a métrica que decide e o resultado que a
derruba. Nenhuma hipótese entra em decisão de arquitetura antes de ser medida.

Regra de leitura: números marcados como aritmética são calculáveis e verificáveis agora; números
marcados como `[INFERENCE]` são estimativas e não valem como evidência até a síntese.

## Restrições já estabelecidas

Fatos da revisão do módulo `leitura_nucleo_setun`, que valem independentemente das hipóteses:

| ID | Restrição | Origem |
| :-- | :-- | :-- |
| C1 | A tabela verdade do decodificador de dois rails está correta: `(1,0)` = +1, `(0,1)` = -1, `(0,0)` = 0 e `(1,1)` = 0 por cancelamento aritmético em complemento de dois | revisão do módulo |
| C2 | A polaridade lógica do módulo está invertida em relação ao software: `src/rebel6/encoding.rs` grava `+1` como `01` e `-1` como `10`, e o módulo devolve `+1` como `10` | `set_trit` e `get_trit` |
| C3 | `default: trit_out = 0` é falha silenciosa e precisa de saída de erro explícita | `docs/directives.md`, proibição de falha silenciosa |
| C4 | Dois bits por trit dão 48 bits por palavra de 24 trits, enquanto o conteúdo de informação é $\log_2 3^{24} = 38{,}04$ bits. O desperdício é 20,8% do espaço de estados | aritmética |
| C5 | Uma palavra de 24 trits tem $2^{48} / 3^{24} = 996{,}6$ codificações possíveis por codificação legal | aritmética |

## Placa alvo: Sipeed Tang Nano 20K

A placa encomendada muda o alvo físico do nível 4. Especificações do fabricante:

| Recurso | Valor |
| :-- | :-- |
| FPGA | GW2AR-LV18QN88C8/I7 |
| LUT4 (Look-Up Table de 4 entradas) | 20.736 |
| Flip-flops | 15.552 |
| S-RAM distribuída | 41.472 bits |
| B-SRAM (Block RAM, memória em blocos) | 828 Kbit em 46 blocos, 103,5 KiB |
| SDRAM | 64 Mbit, 8 MiB, 32 bits |
| Multiplicadores 18×18 | 48 |
| PLL (Phase-Locked Loop, laço de fase) | 2 |
| Clock | cristal de 27 MHz e gerador MS5351 |
| Vídeo | HDMI e conector RGB de 40 pinos para LCD |
| Depuração | BL616 com JTAG e UART por USB |
| Armazenamento | flash de 64 Mbit para bitstream e slot de cartão TF |

### Orçamento de memória (aritmética)

| Item | Tamanho | Onde cabe |
| :-- | --: | :-- |
| Banco de 729 registradores de 24 trits, 2 bits por trit | 4.374 B | B-SRAM, 4,1% |
| Framebuffer 320 × 200 com índice de paleta | 64.000 B | B-SRAM, 60,4% |
| Banco mais framebuffer | 68.374 B | 64,5% da B-SRAM, sobram 37,6 KiB para caches |
| Imagem do guest Doom, 75.515 instruções de 32 trits | 604.120 B | não cabe na B-SRAM, que é 5,7 vezes menor |
| Código, dados e BSS do Doom | 4,96 MiB | SDRAM de 8 MiB, 62,1% |

Consequência imediata: o Doom não roda a partir da B-SRAM nesta placa. Ele exige a SDRAM de 8 MiB,
um controlador de SDRAM e uma cache de instruções. A carga pequena, Dhrystone, CoreMark e o
programa de nível 5, cabe inteira na B-SRAM.

### O que a placa não tem

| Ausência | Efeito |
| :-- | :-- |
| DAC de VGA | O roadmap fala em rasterização por DAC de VGA; a placa oferece HDMI por TMDS (Transition-Minimized Differential Signaling) e RGB paralelo para LCD |
| Medição de corrente | H9 continua exigindo instrumento externo, com shunt na alimentação ou sonda |
| Alvo iCE40 ou Artix-7 | O nível 4 muda de alvo para GW2A; as hipóteses de área em iCE40 passam a ser limite inferior, não alvo |

## H1: o código canônico custa menos lógica que o código redundante

**Afirmação.** Para uma fatia de 24 trits, o código canônico (`01` = +1, `10` = -1, `00` = 0,
`11` ilegal) usa menos LUT (Look-Up Table, tabela de consulta combinacional) que o código
redundante (`00` e `11` ambos zero), porque o redundante exige canonicalização em toda leitura.

**Teste.** Implementar as duas codificações do mesmo conjunto de operações (`min`, `max`, soma mod
3, negação, igualdade) e sintetizar com `nextpnr` para iCE40 UP5K e com `vivado` para Artix-7, com
as mesmas restrições de clock.

**Métrica.** LUT4, flip-flops, Fmax (frequência máxima de operação) e caminho crítico por operação.

**Derruba a hipótese.** O código redundante empata ou ganha em LUT com Fmax igual.

**Classe.** Corte de cabelo: muda o mapeamento do core, não a ISA.

## H2: a negação custa zero LUT nas duas codificações

**Afirmação.** A negação de um trit é fio puro nas duas codificações: troca dos dois bits no código
canônico, inversão dos dois bits no redundante. O sintetizador absorve as duas no roteamento.

**Teste.** Sintetizar um negador de 24 trits em cada codificação e inspecionar o netlist.

**Métrica.** Contagem de LUT igual a zero e netlist contendo apenas conexões.

**Derruba a hipótese.** O sintetizador insere LUT na negação de alguma das duas.

**Nota.** É o teste literal da afirmação do projeto de que `STI.T` é inversão de fio sem
propagação de carry.

**Classe.** Chapéu.

## H3: a igualdade bit a bit só vale no código canônico

**Afirmação.** Com código canônico, `beq.t` sobre 24 trits é uma redução XOR de 48 bits, sem
normalização. Com dois zeros, a comparação precisa canonicalizar 24 trits antes e custa mais.

**Teste.** Sintetizar o caminho de comparação e desvio nas duas codificações.

**Métrica.** LUT no caminho, Fmax e latência do desvio.

**Derruba a hipótese.** As duas codificações custam o mesmo.

**Classe.** Corte de cabelo.

## H4: o empacotamento denso reduz a memória do banco de registradores

**Afirmação.** O banco de 729 registradores de 24 trits ocupa 34.992 bits em dois bits por trit
(729 × 48). Com cinco trits por byte, a palavra cabe em 40 bits e o banco cai para 29.160 bits,
16,7% menos, ao custo de lógica de desempacotamento.

**Teste.** Implementar três variantes do banco: 48 bits por palavra, 40 bits por palavra com cinco
trits por byte, e 39 bits por palavra com empacotamento binário denso.

**Métrica.** Blocos de BRAM (Block RAM, memória em blocos), LUT de desempacotamento, Fmax.

**Derruba a hipótese.** O custo de desempacotamento anula o ganho de BRAM ou derruba o Fmax.

**Classe.** Corte de cabelo.

## H5: o banco de registradores não cabe em flip-flops no iCE40

**Afirmação.** 34.992 bits não cabem nos flip-flops do iCE40 UP5K, que tem cerca de 5.280 células
lógicas, então o banco precisa de BRAM. A BRAM tem duas portas, e o core lê dois registradores e
escreve um por instrução, o que força duplicação de memória ou escrita em dois ciclos.

**Teste.** Sintetizar o banco com duas leituras e uma escrita para UP5K e Artix-7, com e sem
duplicação de BRAM.

**Métrica.** Blocos de BRAM, Fmax, ciclos por escrita.

**Derruba a hipótese.** Cabe em flip-flops, ou as portas bastam sem duplicação nem ciclo extra.

**Classe.** Tatuagem: define o alvo físico do nível 4. `[INFERENCE]` para o número de portas.

## H6: a detecção de erro por trit é viável no orçamento de lógica

**Afirmação.** Com dois bits por trit, `11` sobra como código de erro e a detecção custa um OU por
trit. A alternativa, paridade por palavra, custa menos lógica e cobre menos casos.

**Teste.** Sintetizar as duas variantes sobre o banco de registradores e enumerar quais inversões
de bit cada uma detecta.

**Métrica.** LUT, cobertura de inversão de bit único, custo de escrita.

**Derruba a hipótese.** A paridade por palavra cobre os mesmos casos com menos lógica.

**Classe.** Corte de cabelo.

## H7: o somador ternário direto vence o decodifica, soma e codifica

**Afirmação.** Um somador de 24 trits com carry em $\{-1, 0, 1\}$ implementado direto em LUT vence
a variante que decodifica para binário, soma e recodifica.

**Teste.** Três implementações do mesmo somador: carry ternário direto, decodifica e recodifica, e
ripple sobre trits codificados em binário.

**Métrica.** LUT, Fmax, caminho crítico.

**Derruba a hipótese.** Decodifica e recodifica vence em área e em Fmax.

**Classe.** Corte de cabelo.

## H8: o código redundante é incompatível com lógica assíncrona

**Afirmação.** Em lógica assíncrona dual-rail, `(0,0)` é o espaçador NULL e `(1,1)` é falha.
Tratar `(1,1)` como zero elimina a única detecção que a codificação oferece, então uma tentativa
assíncrona exige o código canônico.

**Teste.** Construir um estágio de pipeline assíncrono de um trit nas duas codificações e observar
handshake e hazards em simulação e em síntese.

**Métrica.** Comportamento de reconhecimento e de falha, LUT, latência.

**Derruba a hipótese.** A codificação redundante preserva as semânticas de NULL e de falha.

**Classe.** Chapéu enquanto a decisão assíncrona não existir.

## H9: a energia do FPGA não é derivável da métrica de software

**Afirmação.** A contagem de trits alterados do emulador não se converte em energia de FPGA sem
medição, e o fator de correlação é desconhecido.

**Teste.** Medir corrente dinâmica na placa para duas cargas, o kernel de Doom e o Dhrystone, e
comparar com a métrica de software nas mesmas cargas.

**Métrica.** Fator entre trits alterados e energia medida, e a dispersão desse fator entre cargas.

**Derruba a hipótese.** Um fator linear estável entre cargas.

**Restrição.** Enquanto não houver medição, nenhum documento do projeto pode afirmar economia de
energia no FPGA. Vale a regra de primazia da realidade de `docs/directives.md`.

**Classe.** Tatuagem: uma afirmação pública de energia é cara de retirar.

## H10: as duas realizações do zero do modelo de ferrite carregam informação útil

**Afirmação.** A redundância do Setun nasceu dos dois núcleos de ferrite com enrolamentos opostos.
Ela só tem valor funcional no FPGA se contar qual das duas realizações ocorre mudar alguma decisão
de projeto.

**Teste.** Instrumentar o emulador de 21 trits e o core de 24 trits para registrar qual realização
do zero o modelo de ferrite produziria em cada operação, e analisar a distribuição nas cargas
existentes (Doom, Dhrystone, suíte RV32I).

**Métrica.** Distribuição das realizações do zero e se ela distingue cargas ou operações.

**Derruba a hipótese.** A distribuição é degenerada, sempre a mesma realização, e a redundância não
informa nada.

**Classe.** Chapéu.

## H11: o fluxo aberto reproduz o fluxo do fornecedor no que o projeto usa

**Afirmação.** O fluxo aberto com Apicula e `nextpnr-gowin` sintetiza o core para GW2A-18 com
B-SRAM, PLL e saída HDMI sem divergência material em área, Fmax e comportamento.

**Teste.** Sintetizar o mesmo desenho no Gowin EDA e no fluxo aberto, com as mesmas restrições.

**Métrica.** LUT, B-SRAM, Fmax, e igualdade funcional em simulação com os mesmos vetores.

**Derruba a hipótese.** O fluxo aberto falha em B-SRAM, PLL ou TMDS, ou diverge de forma que impeça
build reprodutível no CI.

**Classe.** Corte de cabelo: decide se o CI do nível 4 é reprodutível sem ferramenta proprietária.

## H12: o Doom roda a partir da SDRAM acima da velocidade do emulador

**Afirmação.** Com a imagem na SDRAM e uma cache de instruções pequena na B-SRAM, o core sustenta
taxa útil bem acima dos 563 mil instruções por segundo do interpretador em Rust.

**Teste.** Core com controlador de SDRAM e cache de 16 KiB, executando a imagem do Doom; medir
tempo até o frame 2 e taxa de acerto da cache.

**Métrica.** Instruções por segundo, ciclos por instrução, tempo até o frame 2.

**Derruba a hipótese.** A taxa cai para a ordem do emulador, ou o tempo até o frame 2 fica acima de
dez segundos.

**Previsão.** A 100 MHz com seis ciclos por instrução, 16,7 milhões de instruções por segundo, cerca
de 30 vezes o emulador, e frame 2 em 1,9 s. `[INFERENCE]`

**Classe.** Corte de cabelo.

## H13: a telemetria de trits alterados não derruba o Fmax

**Afirmação.** Contar trits alterados em cada escrita de registrador, com XOR de 48 bits e
`popcount`, cabe no caminho crítico a 100 MHz.

**Teste.** Sintetizar o core com e sem o contador e comparar Fmax e área.

**Métrica.** Fmax com e sem o contador, LUT do contador, e custo por escrita.

**Derruba a hipótese.** O contador derruba o Fmax abaixo do alvo, e a telemetria precisa de modo de
medição separado, como os modos veracidade e rápido do software.

**Classe.** Chapéu.

## Ordem de execução por custo

| Ordem | Hipótese | Custo do teste |
| --: | :-- | :-- |
| 1 | H2, negação sem LUT | minutos, um módulo e uma síntese |
| 2 | H1, custo das codificações | horas, dois conjuntos de operações |
| 3 | H3, igualdade | horas |
| 4 | H7, somador | um dia |
| 5 | H6, detecção de erro | um dia |
| 6 | H5, banco em BRAM | um dia, depende de H4 |
| 7 | H4, empacotamento denso | um dia |
| 8 | H8, assíncrono | dias, só se a decisão existir |
| 9 | H10, realizações do zero | dias, trabalho de emulador |
| 10 | H11, fluxo aberto | horas, decide o CI do nível 4 |
| 11 | H13, telemetria no caminho crítico | horas, com o core sintetizado |
| 12 | H12, Doom pela SDRAM | semanas, depende do controlador de SDRAM |
| 13 | H9, energia | exige placa e instrumento |

## O que este documento decide agora

Uma decisão sai daqui antes da síntese: o código de 2 bits do nível 4 precisa ser o mesmo de
`src/rebel6/encoding.rs`, com `11` reservado para erro. A razão está em C2, C3 e H3, e ela não
depende de medir área. Se H1 ou H3 forem derrubadas, a decisão é revisitada com número em mão.
